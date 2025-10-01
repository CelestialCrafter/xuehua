use std::{
    fmt::Debug,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use log::trace;
use mlua::{AnyUserData, ExternalResult, FromLua, IntoLua, Lua, Table, UserData};
use petgraph::graph::NodeIndex;
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::{
    executor::{Executor, MODULE_NAME},
    package::Package,
};

pub struct BuildInfo {
    pub node: NodeIndex,
    pub package: Package,
    pub runtime: Vec<NodeIndex>,
    pub buildtime: Vec<NodeIndex>,
}

pub struct LuaExecutor<E>(Arc<Semaphore>, E);

impl<E> UserData for LuaExecutor<E>
where
    E: Executor + Send + 'static,
    E::Request: FromLua + IntoLua + Debug + Send,
    E::Response: IntoLua + Debug,
{
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("create", |lua, value| E::Request::from_lua(value, lua));

        methods.add_async_method_mut("dispatch", async |_, mut this, request: AnyUserData| {
            let semaphore = this.0.clone();
            let _permit = semaphore.acquire().await.into_lua_err()?;
            let request = request.take()?;

            trace!("dispatching request: {request:?}");
            let response = this.1.dispatch(request).await.into_lua_err();
            trace!("received response: {response:?}");

            response
        });
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub struct Builder<'a> {
    root: &'a Path,
    lua: &'a Lua,
    executors: Vec<(
        String,
        Box<dyn Fn(&Lua, Arc<Path>) -> Result<AnyUserData, mlua::Error>>,
    )>,
}

impl<'a> Builder<'a> {
    pub fn new(root: &'a Path, lua: &'a Lua) -> Self {
        Self {
            root,
            lua,
            executors: Default::default(),
        }
    }

    pub fn register<F, E>(mut self, name: String, concurrent: usize, func: F) -> Self
    where
        F: Fn(Arc<Path>) -> E + Send + 'static,
        E: Executor + Send + 'static,
        E::Request: FromLua + IntoLua + Send + Debug,
        E::Response: IntoLua + Send + Debug,
    {
        let semaphore = Arc::new(Semaphore::new(concurrent));
        let wrapped = move |lua: &Lua, environment| {
            let executor = func(environment);
            lua.create_userdata(LuaExecutor(semaphore.clone(), executor))
        };

        self.executors.push((name, Box::new(wrapped)));
        self
    }

    fn create(&self, lua: &Lua, environment: PathBuf) -> Result<Table, mlua::Error> {
        let environment: Arc<Path> = Arc::from(environment);
        let iter = self
            .executors
            .iter()
            .map(|(name, func)| Ok((name.clone(), func(lua, environment.clone())?)))
            .collect::<Result<Vec<_>, mlua::Error>>()?;
        lua.create_table_from(iter)
    }

    fn environment_dir(&self, node: NodeIndex) -> PathBuf {
        self.root.join(node.index().to_string())
    }

    pub async fn build(&self, info: &BuildInfo) -> Result<(), Error> {
        // create environment
        // TODO: link dependencies
        let environment = self.environment_dir(info.node);
        fs::create_dir(&environment)?;

        // register executors
        let executors = self.create(self.lua, environment)?;
        self.lua.register_module(MODULE_NAME, &executors)?;

        // build pkg
        info.package.build().await?;

        // cleanup
        executors.for_each::<String, AnyUserData>(|_, executor| executor.destroy())?;

        Ok(())
    }
}

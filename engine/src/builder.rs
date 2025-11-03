use std::{fmt::Debug, fs, io, path::Path, sync::Arc};

use log::trace;
use mlua::{AnyUserData, ExternalResult, FromLua, IntoLua, Lua, UserData};
use petgraph::graph::NodeIndex;
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::{executor::Executor, package::Package};

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
        Box<dyn Fn(&Lua, Arc<Path>) -> Result<AnyUserData, Error>>,
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
        F: Fn(Arc<Path>) -> Result<E, Error> + 'static,
        E: Executor + Send + 'static,
        E::Request: FromLua + IntoLua + Send + Debug,
        E::Response: IntoLua + Send + Debug,
    {
        let wrapped = move |lua: &Lua, environment| {
            let executor = func(environment)?;
            let semaphore = Arc::new(Semaphore::new(concurrent));
            let executor = lua.create_userdata(LuaExecutor(semaphore, executor))?;
            Ok(executor)
        };

        self.executors.push((name, Box::new(wrapped)));
        self
    }

    pub async fn build(&self, info: &BuildInfo) -> Result<(), Error> {
        // create environment
        // TODO: link dependencies
        let environment = self.root.join(info.node.index().to_string());
        fs::create_dir(&environment)?;

        // register executors
        let environment: Arc<Path> = Arc::from(environment);
        let executors = self.lua.create_table_from(
            self.executors
                .iter()
                .map(|(name, func)| {
                    let executor = func(self.lua, environment.clone())?;
                    Ok((name.clone(), executor))
                })
                .collect::<Result<Vec<_>, Error>>()?,
        )?;

        let func = info.package.build();
        if let Some(environment) = func.environment() {
            environment.set("executors", &executors)?;
        }

        // build pkg
        func.call_async::<()>(()).await?;

        // cleanup
        executors.for_each::<String, AnyUserData>(|_, executor| executor.destroy())?;

        Ok(())
    }
}

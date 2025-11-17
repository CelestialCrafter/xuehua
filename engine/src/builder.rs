use std::{
    any::Any,
    fmt::Debug,
    fs, io,
    path::Path,
    sync::{Arc, Weak},
};

use log::trace;
use mlua::{AnyUserData, ExternalResult, FromLua, IntoLua, Lua, UserData};
use petgraph::graph::NodeIndex;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};

use crate::{
    executor::Executor,
    package::Package,
    utils::{BoxDynError, register_local_module},
};

pub struct BuildInfo {
    pub node: NodeIndex,
    pub package: Package,
    pub runtime: Vec<NodeIndex>,
    pub buildtime: Vec<NodeIndex>,
}

pub struct LuaExecutor<E>(Weak<(Semaphore, Mutex<E>)>);

impl<E> UserData for LuaExecutor<E>
where
    E: Executor + Send + Sync + 'static,
    E::Request: FromLua + IntoLua + Debug + Send,
    E::Response: IntoLua + Debug,
{
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("create", |lua, value| E::Request::from_lua(value, lua));

        methods.add_async_method_mut("dispatch", async |_, this, request: AnyUserData| {
            let data = this.0.upgrade().ok_or(mlua::Error::UserDataDestructed)?;
            let mut executor = data.1.lock().await;

            let _permit = data.0.acquire().await.into_lua_err()?;

            let request = request.take()?;
            trace!("dispatching request: {request:?}");
            let response = executor.dispatch(request).await.into_lua_err();
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
    #[error(transparent)]
    ExternalError(#[from] BoxDynError),
}

pub struct Builder<'a> {
    root: &'a Path,
    lua: &'a Lua,
    executors: Vec<(
        String,
        Box<dyn Fn(&Lua, Arc<Path>) -> Result<(Arc<dyn Any>, AnyUserData), Error>>,
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

    pub fn register_module(lua: &Lua) -> Result<(), mlua::Error> {
        register_local_module(lua, "xuehua.executors", "__executors")
    }

    pub fn register<F, E>(mut self, name: String, concurrent: usize, func: F) -> Self
    where
        F: Fn(Arc<Path>) -> Result<E, Error> + 'static,
        E: Executor + Send + Sync + 'static,
        E::Request: FromLua + IntoLua + Send + Debug,
        E::Response: IntoLua + Send + Debug,
    {
        let wrapped = move |lua: &Lua, environment| {
            let executor = Mutex::new(func(environment)?);
            let semaphore = Semaphore::new(concurrent);

            let data = Arc::new((semaphore, executor));
            let weak = Arc::downgrade(&data);

            let executor = lua.create_userdata(LuaExecutor(weak))?;
            Ok((data as Arc<dyn Any>, executor))
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
        let (_handles, executors): (Vec<_>, Vec<_>) = self
            .executors
            .iter()
            .map(|(name, func)| {
                let (handle, executor) = func(self.lua, environment.clone())?;
                Ok((handle, (name.clone(), executor)))
            })
            .collect::<Result<Vec<_>, Error>>()?
            .into_iter()
            .unzip();
        let executors = self.lua.create_table_from(executors)?;

        let func = info.package.build();
        if let Some(environment) = func.environment() {
            environment.set("__executors", &executors)?;
        }

        // build pkg
        func.call_async::<()>(()).await?;

        Ok(())
    }
}

use std::{
    fs, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
};

use frunk_core::hlist::{HCons, HList, HNil};
use mlua::{AnyUserData, ExternalResult, FromLua, IntoLua, Lua, UserData};
use petgraph::graph::NodeIndex;
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::{executor::Executor, package::Package, utils::scope::LuaScope};

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
    E::Request: FromLua + IntoLua,
    E::Response: IntoLua,
{
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("create", |lua, value| E::Request::from_lua(value, lua));

        methods.add_async_method_mut("dispatch", async |_, mut this, request: AnyUserData| {
            let _ = this.0.acquire().await.into_lua_err()?;
            let request = request.take()?;
            this.1.dispatch(request).await.into_lua_err()
        });
    }
}

trait PopulateScope<'a, T> {
    fn populate(environment: Arc<Path>) -> Result<LuaScope<'a, T>, mlua::Error>;
}

impl<'a, T> PopulateScope<'a, T> for HNil {
    fn populate(self, _environment: Arc<Path>) -> Result<LuaScope<'a, T>, mlua::Error> {
        Ok(scope)
    }
}

impl<'a, E, F> PopulateScope<'a, F> for HCons<F, HNil>
where
    T: PopulateScope,
    F: Fn(Arc<Path>) -> E + Send + 'static,
    E: Executor + Send + 'static,
    E::Request: FromLua + IntoLua,
    E::Response: IntoLua,
{
    fn populate(self, environment: Arc<Path>) -> Result<LuaScope<'a, HCons<E, HNil>>, mlua::Error> {
        let scope = self.tail.populate(environment.clone())?;
        let environment = self.head(environment);
        scope.push_data(&name, lua_executor)
    }
}

impl<'a, E, H, O> PopulateScope<'a, E> for HCons<H, T>
where
    T: PopulateScope,
    H: Fn(Arc<Path>) -> E + Send + 'static,
    E: Executor + Send + 'static,
    E::Request: FromLua + IntoLua,
    E::Response: IntoLua,
{
    fn populate(self, environment: Arc<Path>) -> Result<LuaScope<'a, HCons<E, O>>, mlua::Error> {
        let scope = self.tail.populate(environment.clone())?;
        let environment = self.head(environment);
        scope.push_data(&name, lua_executor)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub struct Builder<'a, T> {
    root: &'a Path,
    lua: &'a Lua,
    executors: T,
}

impl<'a> Builder<'a, HNil> {
    pub fn new(root: &'a Path, lua: &'a Lua) -> Self {
        Self {
            root,
            lua,
            executors: HNil,
        }
    }
}

impl<'a, T: HList> Builder<'a, T> {
    pub fn register<F, E>(
        self,
        name: String,
        concurrent: usize,
        func: F,
    ) -> Builder<'a, HCons<(String, Arc<Semaphore>, F, PhantomData<E>), T>>
    where
        F: Fn(Arc<Path>) -> E + Send + 'static,
        E: Executor + Send + 'static,
    {
        Builder {
            root: self.root,
            lua: self.lua,
            executors: HCons {
                head: (
                    name,
                    Arc::new(Semaphore::new(concurrent)),
                    func,
                    PhantomData,
                ),
                tail: self.executors,
            },
        }
    }
}

// The trait bound here is now simple and clean.
impl<'a, T: PopulateScope<'a>> Builder<'a, T> {
    pub async fn build(self, info: &BuildInfo) -> Result<(), Error> {
        // create environment
        // TODO: link dependencies
        let environment = self.root.join(info.node.index().to_string());
        fs::create_dir(&environment)?;

        let func = info.package.build();

        let scope = LuaScope::from_function(self.lua, &func)?;
        let scope = self.executors.populate(scope, environment.into())?;

        func.call_async::<()>(()).await?;

        scope.release()?;
        Ok(())
    }
}

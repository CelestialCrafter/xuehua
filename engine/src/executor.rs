pub mod runner;

use futures_util::future::BoxFuture;
#[cfg(feature = "bubblewrap-executor")]
pub use runner::bubblewrap;

use std::{collections::HashMap, path::Path};

use mlua::{AnyUserData, Lua, MultiValue};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("executor {0} not found")]
    ExecutorNotFound(String),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
    #[error(transparent)]
    ExternalError(#[from] Box<dyn std::error::Error + Send + Sync>),
}

// TODO: add examples for executor implementation and usage
/// A controlled gateway for executing side-effects of a package build
///
/// An [`Executor`] is the bridge between an isolated and pure [`Package`](crate::package::Package) definition,
/// and messy real-world actions package builds need to do.
/// Its responsibility is to provide a secure, isolated, and reproducable environment for package builds to actually do things.
///
/// By nature, executors are full of side effects (fetching data, running processes, creating files, etc),
/// but they must strive to be deterministic.
pub trait Executor {
    fn create(&self, lua: &Lua, value: MultiValue) -> Result<AnyUserData, Error>;
    fn dispatch(&'_ mut self, lua: Lua, data: AnyUserData) -> BoxFuture<'_, Result<MultiValue, Error>>;
}

type ExecFuncReturn = Result<Box<dyn Executor + Send>, Error>;

#[derive(Default)]
pub struct Manager(HashMap<String, Box<dyn Fn(&Path) -> ExecFuncReturn>>);

impl<'a> Manager {
    pub fn register<F: Fn(&Path) -> ExecFuncReturn + 'static>(&mut self, name: String, func: F) {
        self.0.insert(name, Box::new(func));
    }

    pub fn new(&self, name: &str, environment: &Path) -> Option<ExecFuncReturn> {
        self.0.get(name).map(|func| func(environment))
    }

    pub fn registered(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(|v| v.as_str())
    }
}

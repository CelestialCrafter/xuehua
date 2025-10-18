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

pub type DynBoxExecutor = Box<dyn Executor + Send + Sync>;

#[derive(Default)]
pub struct Manager {
    registered: HashMap<String, Box<dyn Fn(&Path) -> DynBoxExecutor>>,
}

impl<'a> Manager {
    pub fn register<F: Fn(&Path) -> DynBoxExecutor + 'static>(&mut self, name: String, func: F) {
        self.registered.insert(name, Box::new(func));
    }

    pub fn create(&self, environment: &Path) -> impl Iterator<Item = (String, DynBoxExecutor)> {
        self.registered
            .iter()
            .map(|(name, func)| (name.clone(), func(environment)))
    }
}

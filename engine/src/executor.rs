pub mod runner;
use std::{collections::HashMap, path::PathBuf};

pub use runner::*;

use mlua::{AnyUserData, ExternalResult, Lua, MultiValue, UserData};
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
    fn dispatch(&mut self, lua: &Lua, data: AnyUserData) -> Result<MultiValue, Error>;
}

type BoxDynExec = Box<dyn Executor + Send>;

type ExecFuncReturn = Result<BoxDynExec, Error>;

#[derive(Default)]
pub struct Manager(HashMap<String, Box<dyn Fn(PathBuf) -> ExecFuncReturn>>);

impl<'a> Manager {
    pub fn register<F: Fn(PathBuf) -> ExecFuncReturn + 'static>(&mut self, name: String, func: F) {
        self.0.insert(name, Box::new(func));
    }

    pub fn create(&self, name: &str, environment: PathBuf) -> ExecFuncReturn {
        self.0
            .get(name)
            .ok_or(Error::ExecutorNotFound(name.to_string()))?(environment)
    }

    pub fn registered(&'a self) -> Vec<&'a str> {
        self.0.keys().map(|v| v.as_str()).collect()
    }
}

pub struct LuaExecutor(pub BoxDynExec);

impl UserData for LuaExecutor {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("create", |lua, this, args| {
            this.0.create(lua, args).into_lua_err()
        });

        methods.add_method_mut("dispatch", |lua, this, args| {
            this.0.dispatch(lua, args).into_lua_err()
        });
    }
}

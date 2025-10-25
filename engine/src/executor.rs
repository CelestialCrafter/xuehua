pub mod runner;

#[cfg(feature = "bubblewrap-executor")]
pub use runner::bubblewrap;

use crate::utils::BoxDynError;

use mlua::{AnyUserData, ExternalResult, FromLua, IntoLua, UserData};
use thiserror::Error;

pub const MODULE_NAME: &str = "xuehua.executor";

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
    #[error(transparent)]
    ExternalError(#[from] BoxDynError),
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
pub trait Executor: Sized {
    type Request;
    type Response;

    fn dispatch(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<Self::Response, Error>> + Send;
}

pub struct LuaExecutor<E: Executor>(pub E);

impl<E> UserData for LuaExecutor<E>
where
    E: Executor + Send + 'static,
    E::Request: FromLua + IntoLua,
    E::Response: IntoLua,
{
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("create", |lua, value| E::Request::from_lua(value, lua));

        methods.add_async_method_mut("dispatch", async |_, mut this, request: AnyUserData| {
            let request = request.take()?;
            this.0.dispatch(request).await.into_lua_err()
        });
    }
}

#[cfg(feature = "http-executor")]
pub mod http;
pub mod runner;

#[cfg(feature = "bubblewrap-executor")]
pub use runner::bubblewrap;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::{backend::Backend, utils::BoxDynError};

#[derive(Error, Debug)]
pub enum Error<B: Backend> {
    #[error("executor {0:?} not found")]
    NotFound(String),
    // fluff my chungy trait solver
    #[error(transparent)]
    BackendError(B::Error),
    #[error(transparent)]
    ExecutorError(#[from] BoxDynError),
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
pub trait Executor: Send + Sized {
    const NAME: &'static str;
    type Request: DeserializeOwned;
    type Error: std::error::Error + Send + Sync + 'static;

    fn execute(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

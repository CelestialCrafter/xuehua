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

pub trait Executor: Send + Sized {
    const NAME: &'static str;
    type Request: DeserializeOwned;
    type Error: std::error::Error + Send + Sync + 'static;

    fn execute(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

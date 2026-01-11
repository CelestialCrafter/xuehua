use std::{fs::create_dir, path::PathBuf, sync::Arc};

use futures_util::{FutureExt, future::BoxFuture};
use petgraph::graph::NodeIndex;
use smol_str::SmolStr;
use thiserror::Error;
use xh_archive::{
    Event,
    packing::{Error as PackingError, Packer},
};

use crate::{
    backend::Backend,
    executor::Executor,
    package::DispatchRequest,
    planner::{Frozen, Planner},
    utils::BoxDynError,
};

#[derive(Error, Debug)]
pub enum Error<B: Backend> {
    #[error("executor {0} not found")]
    UnregisteredExecutor(SmolStr),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    PackingError(#[from] PackingError),
    #[error(transparent)]
    BackendError(B::Error),
    #[error(transparent)]
    ExecutorError(#[from] BoxDynError),
}

pub type BuildId = u64;

#[derive(Debug, Clone, Copy)]
pub struct BuildRequest {
    pub id: BuildId,
    pub target: NodeIndex,
}

#[derive(Debug, Clone)]
pub struct InitializeContext {
    pub environment: PathBuf,
}

#[derive(Clone)]
pub struct ExecutorPair<E>(E);

pub trait Initialize
where
    Self: Sized,
{
    type Output;

    fn initialize(&self, ctx: Arc<InitializeContext>) -> Result<Self::Output, BoxDynError>;
}

impl Initialize for ExecutorPair<()> {
    type Output = ExecutorPair<()>;

    fn initialize(&self, _ctx: Arc<InitializeContext>) -> Result<Self::Output, BoxDynError> {
        Ok(ExecutorPair(()))
    }
}

impl<E, F, T> Initialize for ExecutorPair<(F, T)>
where
    F: Fn(Arc<InitializeContext>) -> Result<E, BoxDynError>,
    T: Initialize,
{
    type Output = ExecutorPair<(E, T::Output)>;

    fn initialize(&self, ctx: Arc<InitializeContext>) -> Result<Self::Output, BoxDynError> {
        let (head, tail) = &self.0;
        Ok(ExecutorPair((head(ctx.clone())?, tail.initialize(ctx)?)))
    }
}

type DispatchResult<'a, B> = Option<BoxFuture<'a, Result<(), Error<B>>>>;

pub trait Dispatch<B: Backend> {
    fn dispatch<'a>(
        &'a mut self,
        backend: &'a B,
        request: &DispatchRequest<B>,
    ) -> DispatchResult<'a, B>;
}

impl<B: Backend + Send + Sync> Dispatch<B> for ExecutorPair<()> {
    fn dispatch<'a>(
        &'a mut self,
        _backend: &'a B,
        _request: &DispatchRequest<B>,
    ) -> DispatchResult<'a, B> {
        None
    }
}

impl<E, T, B> Dispatch<B> for ExecutorPair<(E, T)>
where
    B: Backend + Send + Sync,
    T: Dispatch<B> + Send,
    E: Executor,
{
    fn dispatch<'a>(
        &'a mut self,
        backend: &'a B,
        request: &DispatchRequest<B>,
    ) -> DispatchResult<'a, B> {
        if E::NAME == request.executor {
            let payload = request.payload.clone();
            let future = (async move || {
                let payload = match backend.deserialize(payload).map_err(Error::BackendError) {
                    Ok(v) => v,
                    Err(err) => return Err(err),
                };

                let executor = &mut self.0.0;
                executor
                    .execute(payload)
                    .await
                    .map_err(BoxDynError::from)
                    .map_err(Into::into)
            })();

            Some(future.boxed())
        } else {
            self.0.1.dispatch(backend, request)
        }
    }
}

pub struct Builder<B, T> {
    pub root: PathBuf,
    pub backend: Arc<B>,
    pub executors: T,
}

impl<B: Backend> Builder<B, ExecutorPair<()>> {
    pub fn new(root: PathBuf, backend: Arc<B>) -> Self {
        Self {
            root,
            backend,
            executors: ExecutorPair(()),
        }
    }
}

impl<B, T> Builder<B, T>
where
    B: Backend,
    T: Initialize,
    T::Output: Dispatch<B>,
{
    pub fn register<E, F>(self, init: F) -> Builder<B, ExecutorPair<(F, T)>>
    where
        E: Executor,
        F: Fn(Arc<InitializeContext>) -> Result<E, BoxDynError>,
    {
        Builder {
            root: self.root,
            backend: self.backend,
            executors: ExecutorPair((init, self.executors)),
        }
    }

    fn environment_path(&self, id: &BuildId) -> PathBuf {
        self.root.join(id.to_string())
    }

    pub fn fetch(&self, build: &BuildId) -> Result<Option<Vec<Event>>, Error<B>> {
        let output = self.environment_path(build).join("output");
        if !std::fs::exists(&output)? {
            return Ok(None);
        }

        let mut packer = Packer::new(output);
        let archive = unsafe { packer.pack_mmap_iter() }.collect::<Result<Vec<_>, _>>()?;

        Ok(Some(archive))
    }

    pub async fn build(
        &self,
        planner: &Planner<Frozen<'_, B>>,
        request: BuildRequest,
    ) -> Result<(), Error<B>> {
        let environment = self.environment_path(&request.id);
        create_dir(&environment)?;
        create_dir(environment.join("output"))?;

        // TODO: link closure
        // planner.closure(request.target);

        let mut executors = self
            .executors
            .initialize(InitializeContext { environment }.into())?;

        for request in &planner.graph()[request.target].requests {
            executors
                .dispatch(&self.backend, request)
                .ok_or_else(|| Error::UnregisteredExecutor(request.executor.clone()))?
                .await?;
        }

        Ok(())
    }
}

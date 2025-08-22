use std::{fs::create_dir, path::PathBuf, sync::Arc};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use petgraph::graph::NodeIndex;
use serde::Deserialize;
use smol_str::SmolStr;
use xh_archive::{Event, packing::Packer};
use xh_reports::{compat::StdCompat, prelude::*};

use crate::{executor::Executor, package::DispatchRequest, planner::{Frozen, Planner}};

#[derive(Debug, IntoReport)]
#[message("executor not found")]
#[suggestion("provide a registered executor")]
#[context(name)]
pub struct UnregisteredExecutorError {
    pub name: SmolStr,
}

#[derive(Default, Debug, IntoReport)]
#[message("could not initialize executor")]
pub struct InitializationError;

#[derive(Default, Debug, IntoReport)]
#[message("could not build package")]
pub struct Error;

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

    fn initialize(&self, ctx: Arc<InitializeContext>) -> Result<Self::Output, InitializationError>;
}

impl Initialize for ExecutorPair<()> {
    type Output = ExecutorPair<()>;

    fn initialize(
        &self,
        _ctx: Arc<InitializeContext>,
    ) -> Result<Self::Output, InitializationError> {
        Ok(ExecutorPair(()))
    }
}

impl<E, F, T> Initialize for ExecutorPair<(F, T)>
where
    F: Fn(Arc<InitializeContext>) -> Result<E, InitializationError>,
    T: Initialize,
{
    type Output = ExecutorPair<(E, T::Output)>;

    fn initialize(&self, ctx: Arc<InitializeContext>) -> Result<Self::Output, InitializationError> {
        let (head, tail) = &self.0;
        Ok(ExecutorPair((head(ctx.clone())?, tail.initialize(ctx)?)))
    }
}

type DispatchResult<'a> = Option<BoxFuture<'a, Result<(), Error>>>;

pub trait Dispatch {
    fn dispatch(&mut self, request: &DispatchRequest) -> DispatchResult<'_>;
}

impl Dispatch for ExecutorPair<()> {
    fn dispatch(&mut self, _request: &DispatchRequest) -> DispatchResult<'_> {
        None
    }
}

impl<E, T> Dispatch for ExecutorPair<(E, T)>
where
    T: Dispatch + Send,
    E: Executor,
    E::Request: Send,
{
    fn dispatch(&mut self, request: &DispatchRequest) -> DispatchResult<'_> {
        if E::NAME == request.executor {
            let payload = E::Request::deserialize(request.payload.clone());
            Some(async { self.0.0.execute(payload.wrap()?).await.wrap() }.boxed())
        } else {
            self.0.1.dispatch(request)
        }
    }
}

pub struct Builder<T> {
    pub root: PathBuf,
    pub executors: T,
}

impl Builder<ExecutorPair<()>> {
    #[inline]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            executors: ExecutorPair(()),
        }
    }
}

impl<T> Builder<T>
where
    T: Initialize,
    T::Output: Dispatch,
{
    pub fn register<E, F>(self, init: F) -> Builder<ExecutorPair<(F, T)>>
    where
        E: Executor,
        F: Fn(Arc<InitializeContext>) -> Result<E, InitializationError>,
    {
        Builder {
            root: self.root,
            executors: ExecutorPair((init, self.executors)),
        }
    }

    fn environment_path(&self, id: &BuildId) -> PathBuf {
        self.root.join(id.to_string())
    }

    pub fn fetch(&self, build: &BuildId) -> Result<Option<Vec<Event>>, Error> {
        let output = self.environment_path(build).join("output");
        if !std::fs::exists(&output).compat().wrap()? {
            return Ok(None);
        }

        let mut packer = Packer::new(output);
        let archive = unsafe { packer.pack_mmap_iter() }
            .collect::<Result<Vec<_>, _>>()
            .wrap()?;

        Ok(Some(archive))
    }

    pub async fn build(&self, planner: &Planner<Frozen>, request: BuildRequest) -> Result<(), Error> {
        let environment = self.environment_path(&request.id);

        create_dir(&environment)
            .and_then(|()| create_dir(environment.join("output")))
            .compat()
            .wrap()?;

        // TODO: link closure
        // planner.closure(request.target);

        let mut executors = self
            .executors
            .initialize(InitializeContext { environment }.into())
            .wrap()?;

        for request in &planner.graph()[request.target].requests {
            executors
                .dispatch(request)
                .ok_or_else(|| {
                    UnregisteredExecutorError {
                        name: request.executor.clone(),
                    }
                    .wrap()
                })?
                .await?;
        }

        Ok(())
    }
}

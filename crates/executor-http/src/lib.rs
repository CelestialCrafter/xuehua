use std::{
    path::{Component, PathBuf},
    sync::{Arc, LazyLock},
};

use serde::{Deserialize, Serialize};
use ureq::{
    Agent,
    config::Config,
    http::{Method, Request as HttpRequest, Uri},
};
use xh_engine::{builder::InitializeContext, executor::{Error, Executor}, gen_name, name::ExecutorName};
use xh_reports::prelude::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub path: PathBuf,
    #[serde(with = "xh_common::serde_display")]
    pub url: Uri,
    #[serde(with = "xh_common::serde_display")]
    pub method: Method,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            user_agent: default_user_agent(),
        }
    }
}

fn default_user_agent() -> String {
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")).to_string()
}

#[derive(Debug)]
pub struct HttpExecutor {
    ctx: Arc<InitializeContext>,
    agent: Agent,
}

impl HttpExecutor {
    #[inline]
    pub fn new(ctx: Arc<InitializeContext>, options: Options) -> Self {
        Self {
            ctx,
            agent: Config::builder()
                .user_agent(options.user_agent)
                .build()
                .new_agent(),
        }
    }
}

#[derive(Debug, IntoReport)]
#[message("paths referencing parent directories are not allowed")]
pub struct InvalidPathError;

impl Executor for HttpExecutor {
    type Request = Request;

    fn name() -> &'static ExecutorName {
        static NAME: LazyLock<ExecutorName> = LazyLock::new(|| gen_name!(http@xuehua));
        &NAME
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn execute(&mut self, request: Self::Request) -> Result<(), Error> {

        // TODO: support parent refs
        // crude check to ensure no directory traversals are possible
        if request
            .path
            .components()
            .any(|component| matches!(&component, Component::ParentDir))
        {
            return Err(InvalidPathError.wrap());
        }

        let path = self.ctx.environment.join(request.path);
        let agent = self.agent.clone();

        let span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _guard = span.enter();

            let mut file = std::fs::File::create(path).wrap()?;
            let request = HttpRequest::builder()
                .method(request.method)
                .uri(request.url)
                .body(())
                .wrap()?;

            let response = agent.run(request).wrap()?;
            std::io::copy(&mut response.into_body().as_reader(), &mut file).wrap()?;

            Ok(())
        })
        .await
        .wrap()
        .flatten()
    }
}

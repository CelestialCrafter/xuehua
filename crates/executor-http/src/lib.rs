use std::{
    path::{Component, PathBuf},
    sync::{Arc, LazyLock},
};

use log::debug;
use serde::{Deserialize, Serialize};
use ureq::{
    Agent,
    config::Config,
    http::{Method, Request as HttpRequest, Uri},
};
use xh_engine::{builder::InitializeContext, executor::Executor, gen_name, name::ExecutorName};
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

#[derive(Default, Debug, IntoReport)]
#[message("could not run http executor")]
pub struct Error;

impl Executor for HttpExecutor {
    type Request = Request;
    type Error = Error;

    fn name() -> &'static ExecutorName {
        static NAME: LazyLock<ExecutorName> = LazyLock::new(|| gen_name!(http@xuehua));
        &*NAME
    }

    async fn execute(&mut self, request: Self::Request) -> Result<(), Error> {
        debug!("making request to {}", request.url);

        // TODO: support parent refs
        // crude check to ensure no directory traversals are possible
        if request
            .path
            .components()
            .find(|component| matches!(component, Component::ParentDir))
            .is_some()
        {
            return Err(InvalidPathError.wrap());
        }

        let path = self.ctx.environment.join(request.path);
        let agent = self.agent.clone();

        tokio::task::spawn_blocking(move || {
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

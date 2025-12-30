use std::{
    io::copy,
    path::{Component, PathBuf},
    str::FromStr,
    sync::Arc,
};

use log::debug;
use mlua::{ExternalResult, FromLua, Table, UserData};
use serde::Deserialize;
use thiserror::Error;
use tokio::{fs::OpenOptions, task::spawn_blocking};
use tokio_util::io::SyncIoBridge;
use ureq::{
    Agent,
    config::Config,
    http::{Method, Request, Uri},
};

use crate::{builder::InitializeContext, executor::Executor};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
pub struct HttpRequest {
    pub path: PathBuf,
    #[serde(with = "http_serde::uri")]
    pub url: Uri,
    #[serde(with = "http_serde::method")]
    pub method: Method,
}

impl UserData for HttpRequest {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("path", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("url", |_, this| Ok(this.url.to_string()));
        fields.add_field_method_get("method", |_, this| Ok(this.method.to_string()));
    }
}

impl FromLua for HttpRequest {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;

        let url = Uri::from_str(&table.get::<String>("url")?).into_lua_err()?;
        let method = Method::from_str(&table.get::<String>("method")?).into_lua_err()?;
        // ensure the method is:
        // 1. a valid method, and not an accidental extension
        // 2. a read-only method, to prevent misuse
        if !method.is_safe() {
            return Err(mlua::Error::external("unsafe request method"));
        }

        Ok(Self {
            path: table.get("path")?,
            url,
            method,
        })
    }
}

pub struct HttpExecutor {
    ctx: Arc<InitializeContext>,
    agent: Agent,
}

impl HttpExecutor {
    pub fn new(ctx: Arc<InitializeContext>) -> Self {
        Self {
            ctx,
            agent: Config::builder().user_agent(USER_AGENT).build().new_agent(),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    RequestError(#[from] ureq::Error),

    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error("paths referencing parent directories are not allowed")]
    InvalidPath,
}

impl Executor for HttpExecutor {
    const NAME: &'static str = "http@xuehua/executors";
    type Request = HttpRequest;
    type Error = Error;

    async fn execute(&mut self, request: Self::Request) -> Result<(), Self::Error> {
        debug!("making request to {}", request.url);
        // TODO: support parent refs
        // crude check to ensure no directory traversals are possible
        if request
            .path
            .components()
            .find(|component| matches!(component, Component::ParentDir))
            .is_some()
        {
            return Err(Error::InvalidPath);
        }

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(self.ctx.environment.join(request.path))
            .await?;
        let mut sync_file = SyncIoBridge::new(file);

        // TODO: switch to hyper if this becomes an issue
        let agent = self.agent.clone();
        spawn_blocking(move || {
            let request = Request::builder()
                .method(request.method)
                .uri(request.url)
                .body(())
                .map_err(ureq::Error::from)?;

            let response = agent.run(request)?;
            copy(&mut response.into_body().into_reader(), &mut sync_file)?;

            Ok::<_, Error>(())
        })
        .await?
    }
}

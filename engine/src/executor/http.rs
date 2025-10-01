use std::{
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use log::debug;
use mlua::{ExternalResult, FromLua, Table, UserData};
use reqwest::{Client, Method, StatusCode, Url};

use crate::{executor::Executor, utils::BoxDynError};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
pub struct HttpRequest {
    pub path: PathBuf,
    pub url: Url,
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

        let method = Method::from_str(&table.get::<String>("method")?).into_lua_err()?;
        // ensure the method is:
        // 1. a valid method, and not an accidental extension
        // 2. a read-only method, to prevent misuse
        if !method.is_safe() {
            return Err(mlua::Error::external("unsafe request method"))
        }

        let url = Url::parse(&table.get::<String>("url")?).into_lua_err()?;

        Ok(Self {
            path: table.get("path")?,
            url,
            method,
        })
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: StatusCode,
}

impl UserData for HttpResponse {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("status", |_, this| Ok(this.status.as_u16()));
    }
}

pub struct HttpExecutor {
    environment: Arc<Path>,
    client: Client,
}

impl HttpExecutor {
    pub fn new(environment: Arc<Path>) -> Result<Self, reqwest::Error> {
        let client = Client::builder().user_agent(USER_AGENT).build()?;

        Ok(Self {
            environment,
            client,
        })
    }
}

impl Executor for HttpExecutor {
    type Request = HttpRequest;

    type Response = HttpResponse;

    async fn dispatch(&mut self, request: Self::Request) -> Result<Self::Response, BoxDynError> {
        debug!("making request to {}", request.url);
        // TODO: support parent refs
        // crude check to ensure no directory traversals are possible
        if request
            .path
            .components()
            .find(|component| matches!(component, Component::ParentDir))
            .is_some()
        {
            return Err("paths referencing parent directories are not allowed".into());
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(self.environment.join(request.path))?;

        let mut resp = self
            .client
            .request(request.method, request.url)
            .send()
            .await?
            .error_for_status()?;

        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk)?;
        }

        Ok(HttpResponse {
            status: resp.status(),
        })
    }
}

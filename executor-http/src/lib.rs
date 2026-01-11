use std::{
    fmt,
    marker::PhantomData,
    path::{Component, PathBuf},
    str::FromStr,
    sync::Arc,
};

use log::debug;
use serde::{
    Deserialize, Deserializer,
    de::{Error as DeError, Visitor},
};
use thiserror::Error;
use ureq::{
    Agent,
    config::Config,
    http::{Method, Request, Uri},
};
use xh_engine::{builder::InitializeContext, executor::Executor};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

// stolen from https://users.rust-lang.org/t/serde-fromstr-on-a-field/99457/5
fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: fmt::Display,
    D: Deserializer<'de>,
{
    struct Helper<S>(PhantomData<S>);
    impl<'de, S> Visitor<'de> for Helper<S>
    where
        S: FromStr,
        S::Err: fmt::Display,
    {
        type Value = S;

        fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "a string")
        }

        fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
            value.parse::<Self::Value>().map_err(DeError::custom)
        }
    }

    deserializer.deserialize_str(Helper(PhantomData))
}

#[derive(Debug, Deserialize)]
pub struct HttpRequest {
    pub path: PathBuf,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub url: Uri,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub method: Method,
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

        let path = self.ctx.environment.join(request.path);
        let agent = self.agent.clone();

        tokio::task::spawn_blocking(move || {
            let mut file = std::fs::File::create(path)?;
            let request = Request::builder()
                .method(request.method)
                .uri(request.url)
                .body(())
                .map_err(ureq::Error::from)?;

            let response = agent.run(request)?;
            std::io::copy(&mut response.into_body().as_reader(), &mut file)?;

            Ok(())
        })
        .await?
    }
}

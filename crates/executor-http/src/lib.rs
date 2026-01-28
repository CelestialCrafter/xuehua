use std::{
    path::{Component, PathBuf},
    sync::{Arc, LazyLock},
};

use log::debug;
use serde::{Deserialize, Serialize};
use ureq::{
    Agent,
    config::Config,
    http::{Method, Request, Uri},
};
use xh_engine::{builder::InitializeContext, executor::Executor, gen_name, name::ExecutorName};
use xh_reports::prelude::*;

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

mod serde_display {
    use std::{fmt, marker::PhantomData, str::FromStr};

    use serde::{
        Deserializer, Serializer,
        de::{Error as DeError, Visitor},
    };

    // stolen from https://users.rust-lang.org/t/serde-fromstr-on-a-field/99457/5
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
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
                value.parse::<Self::Value>().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Helper(PhantomData))
    }

    pub fn serialize<T: fmt::Display, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpRequest {
    pub path: PathBuf,
    #[serde(with = "serde_display")]
    pub url: Uri,
    #[serde(with = "serde_display")]
    pub method: Method,
}

pub struct HttpExecutor {
    ctx: Arc<InitializeContext>,
    agent: Agent,
}

impl HttpExecutor {
    #[inline]
    pub fn new(ctx: Arc<InitializeContext>) -> Self {
        Self {
            ctx,
            agent: Config::builder().user_agent(USER_AGENT).build().new_agent(),
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
    type Request = HttpRequest;
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
            let request = Request::builder()
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

#[cfg(feature = "zstd")]
mod zstd;

use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use serde::{Deserialize, Serialize};

use xh_engine::{builder::InitializeContext, executor::{Error, Executor}, gen_name, name::ExecutorName};

use xh_reports::prelude::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Algorithm {
    #[cfg(feature = "zstd")]
    Zstd,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Compress,
    Decompress,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub algorithm: Algorithm,
    pub action: Action,
    pub input: PathBuf,
    pub output: PathBuf,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct Options {
    #[cfg(feature = "zstd")]
    zstd_level: zstd_safe::CompressionLevel,
}

pub struct CompressionExecutor {
    ctx: Arc<InitializeContext>,
    options: Options,
}

impl CompressionExecutor {
    #[inline]
    pub fn new(ctx: Arc<InitializeContext>, options: Options) -> Self {
        Self { ctx, options }
    }
}

impl Executor for CompressionExecutor {
    type Request = Request;

    fn name() -> &'static ExecutorName {
        static NAME: LazyLock<ExecutorName> = LazyLock::new(|| gen_name!(compression@xuehua));
        &*NAME
    }

    async fn execute(&mut self, request: Self::Request) -> Result<(), Error> {
        let input = xh_common::safe_path(&self.ctx.environment, &request.input).wrap()?;
        let output = xh_common::safe_path(&self.ctx.environment, &request.output).wrap()?;
        let options = self.options.clone();

        tokio::task::spawn_blocking(move || {
            match request.algorithm {
                #[cfg(feature = "zstd")]
                Algorithm::Zstd => match request.action {
                    Action::Compress => zstd::compress(&options, &input, &output),
                    Action::Decompress => zstd::decompress(&options, &input, &output),
                },
            }
        })
        .await
        .erased()
        .flatten()
        .wrap()
    }
}

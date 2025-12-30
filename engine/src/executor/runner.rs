#[cfg(feature = "bubblewrap-executor")]
pub mod bubblewrap;

use std::{process::ExitStatus, string::FromUtf8Error};

use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // TODO: improve this error
    #[error("command exited with code {0:?}")]
    CommandFailed(ExitStatus),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    Utf8Error(#[from] FromUtf8Error),
}

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
pub struct CommandRequest {
    program: String,
    working_dir: Option<String>,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
}

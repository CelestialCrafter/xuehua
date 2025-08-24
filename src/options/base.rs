use serde::Deserialize;
use std::{ffi::OsString, path::PathBuf};

#[derive(Deserialize, Debug)]
pub struct SandboxOptions {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub bwrap_options: Vec<OsString>,
}

impl Default for SandboxOptions {
    fn default() -> Self {
        Self {
            enable: true,
            bwrap_options: vec![],
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct BaseOptions {
    #[serde(default)]
    pub root: PathBuf,
    pub sandbox: SandboxOptions,
}

impl Default for BaseOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            sandbox: SandboxOptions::default(),
        }
    }
}

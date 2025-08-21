use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
pub struct BaseOptions {
    #[serde(default)]
    pub root: PathBuf,
    #[serde(default)]
    pub sandbox: bool,
}

impl Default for BaseOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            sandbox: true,
        }
    }
}

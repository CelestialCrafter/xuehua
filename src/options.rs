use std::{env, fs, path::PathBuf, sync::LazyLock};

use clap::Parser;
use eyre::{Context, OptionExt, Result};

use crate::options::{base::BaseOptions, cli::CliOptions};

pub mod base;
pub mod cli;

pub const DISPLAY_NAME: &str = "Xuěhuā";
pub static OPTIONS: LazyLock<Options> =
    LazyLock::new(|| Options::parse().expect("could not parse options"));

#[derive(Debug)]
pub struct Options {
    pub cli: CliOptions,
    pub base: BaseOptions,
}

fn find_options() -> Result<PathBuf> {
    let mut paths = vec![PathBuf::from("/etc/xuehua/options.toml")];
    if let Ok(home) = env::var("HOME") {
        paths.push(PathBuf::from(home).join(".config/xuehua/options.toml"));
    }

    let not_found_error = format!("searched paths: {paths:?}");

    paths
        .into_iter()
        .find_map(|path| {
            match fs::exists(&path)
                .inspect_err(|err| eprintln!("{}", err))
                .ok()?
            {
                true => Some(path),
                false => None,
            }
        })
        .ok_or_eyre("could not find config file")
        .wrap_err(not_found_error)
}

impl Options {
    fn parse() -> Result<Self> {
        Ok(Options {
            cli: CliOptions::parse(),
            base: match find_options() {
                Ok(path) => toml::from_slice(&fs::read(path)?)?,
                Err(err) => {
                    eprintln!("{err}, continuing without config.");
                    BaseOptions::default()
                }
            },
        })
    }
}

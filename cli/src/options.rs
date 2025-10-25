use std::{env, fs, path::PathBuf, sync::OnceLock};

use clap::Parser;
use eyre::{Context, OptionExt, Result};

pub const DISPLAY_NAME: &str = "Xuěhuā";

static OPTIONS: OnceLock<Options> = OnceLock::new();

pub struct Options {
    pub cli: CliOptions,
}

impl Options {
    fn parse() -> Result<Self> {
        Ok(Self {
            cli: CliOptions::try_parse()?,
        })
    }
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Subcommand {
    /// link a package and its dependencies
    Link {
        /// un-link a previously linked package
        #[arg(long, short)]
        reverse: bool,

        #[arg(value_name = "PACKAGE")]
        package: String,
    },

    /// builds a package
    Build {
        #[arg(value_name = "PACKAGE")]
        package: String,
    },

    /// start a shell in a package's environment
    Shell {
        #[arg(value_name = "PACKAGE")]
        package: String,
    },
    
    // inspect a package or xuehua project
    Inspect {
        #[command(subcommand)]
        subcommand: InspectSub,
    },

    /// remove unlinked packages from the store
    GC,

    /// verifies store contents, and re-links partially linked packages
    Repair,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum InspectSub {
    Plan {
        #[arg(value_name = "PATH")]
        path: String,
    },
    
    Package {
        #[arg(value_name = "PACKAGE")]
        package: String,
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliOptions {
    /// don't make any changes to the system
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub subcommand: Subcommand,
}

fn find_options_file() -> Result<PathBuf> {
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

pub fn get_options<'a>() -> &'a Options {
    OPTIONS.get_or_init(|| Options::parse().expect("could not parse options"))
}

#[cfg(test)]
mod test {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn test_verify_args() {
        CliOptions::command().debug_assert();
    }
}

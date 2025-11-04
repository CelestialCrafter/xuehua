use std::{env, fs, path::PathBuf};

use eyre::{OptionExt, Result, WrapErr};

impl Default for BaseOptions {
    fn default() -> Self {
        Self {
            // TODO: switch default build dir based on user/system
            build_directory: env::temp_dir(),
        }
    }
}

pub fn find_options_file() -> Result<PathBuf> {
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

pub struct BaseOptions {
    pub build_directory: PathBuf,
}

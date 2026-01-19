pub mod base;
pub mod cli;

use xh_reports::prelude::*;

use std::sync::OnceLock;

pub static OPTIONS: OnceLock<Options> = OnceLock::new();

pub fn get_opts() -> &'static Options {
    OPTIONS.get().expect("options should be initialized")
}

pub struct Options {
    pub cli: cli::Options,
    pub base: base::BaseOptions,
}

impl Options {
    pub fn run() -> Result<Self, ()> {
        Ok(Options {
            cli: cli::Options::options().run().into(),
            base: base::BaseOptions::read().erased()?,
        })
    }
}

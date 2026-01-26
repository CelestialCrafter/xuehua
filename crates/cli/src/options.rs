pub mod base;
pub mod cli;

use pico_args::Arguments;
use xh_reports::prelude::*;

use std::sync::OnceLock;

pub static OPTIONS: OnceLock<Options> = OnceLock::new();

pub fn get_opts() -> &'static Options {
    OPTIONS.get().expect("options should be initialized")
}

pub struct Options {
    pub action: cli::Action,
    pub base: base::BaseOptions,
}

impl Options {
    pub fn run() -> Result<Self, ()> {
        Ok(Options {
            action: cli::Action::parse(&mut Arguments::from_env(), &mut Vec::new()).erased()?,
            base: base::BaseOptions::read().erased()?,
        })
    }
}

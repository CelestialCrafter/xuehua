pub mod action;
pub mod base;

use std::sync::OnceLock;

use crate::options::{action::Action, base::BaseOptions};

pub static OPTIONS: OnceLock<Options> = OnceLock::new();

pub fn get_opts() -> &'static Options {
    OPTIONS.get().expect("options should be initialized")
}

pub struct Options {
    pub action: action::Action,
    pub base: base::BaseOptions,
}

impl Options {
    pub fn apply(base: BaseOptions, action: Action) -> Self {
        Options { action, base }
    }
}

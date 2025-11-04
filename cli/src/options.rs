pub mod base;
pub mod cli;

use std::sync::LazyLock;

pub static OPTIONS: LazyLock<Options> = LazyLock::new(|| Options {
    // TODO: use .run_inner() and completely overhaul the display
    cli: cli::Options::options().run(),
    base: base::BaseOptions::default(),
});

pub struct Options {
    pub cli: cli::Options,
    pub base: base::BaseOptions,
}

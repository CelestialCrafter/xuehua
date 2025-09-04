pub mod engine;
pub mod options;
pub mod store;
pub mod utils;

use std::{fs, io::stderr};

use eyre::{Context, DefaultHandler, Result};
use log::LevelFilter;

use crate::
    options::{OPTIONS, cli::Subcommand}
;

fn main() -> Result<()> {
    eyre::set_hook(Box::new(DefaultHandler::default_with))
        .wrap_err("error installing eyre handler")?;

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] {} {}",
                record.level(),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::Debug)
        .chain(stderr())
        .apply()
        .wrap_err("error installing logger")?;

    match &OPTIONS.cli.subcommand {
        Subcommand::Build { package: _ } => {
            engine::run(&fs::read("xuehua/main.lua")?)?;
        }
        Subcommand::Link {
            reverse: _,
            package: _,
        } => todo!("link not yet implemented"),
        Subcommand::Shell { package: _ } => todo!("shell not yet implemented"),
        Subcommand::GC => todo!("gc not yet implemented"),
        Subcommand::Repair => todo!("repair not yet implemented"),
    }

    Ok(())
}

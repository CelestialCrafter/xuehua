pub mod options;

use std::{io::stderr, path::Path};

use eyre::{Context, DefaultHandler, Result};
use log::LevelFilter;
use mlua::Lua;
use petgraph::dot::Dot;
use xh_engine::modules::{
    builder::bubblewrap::BubblewrapBuilder, resolver::Resolver, logger, planner::Planner, utils,
};

use crate::options::{Subcommand, get_options};

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

    match get_options().cli.subcommand {
        Subcommand::Build { package: _ } => {
            // TODO: restrict stdlibs
            let lua = Lua::new();

            // inject apis
            logger::inject(&lua)?;
            utils::inject(&lua)?;

            let linker = Resolver::new(|| BubblewrapBuilder::new(Path::new("")));
            let mut planner = Planner::new();

            planner.run(&lua, Path::new("xuehua/main.lua"))?;
            println!("{:?}", Dot::new(&planner.plan()));
            let output = linker.link(&lua, planner.plan(), 2.into())?;
            println!("{:?}", output);
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

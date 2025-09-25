pub mod options;

use std::{io::stderr, path::Path};

use eyre::{Context, DefaultHandler, Result};
use log::LevelFilter;
use mlua::Lua;
use petgraph::dot::Dot;
use tempfile::tempdir_in;
use xh_engine::{
    builder::Builder,
    executor::bubblewrap::{BubblewrapExecutor, BubblewrapExecutorOptions},
    logger,
    planner::Planner,
    store::local::LocalStore,
    utils,
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

            // setup engine modules
            let store_path = Path::new("store");
            utils::ensure_dir(store_path)?;
            let mut store = LocalStore::new(store_path, false)?;

            let mut planner = Planner::new();
            planner.run(&lua, Path::new("xuehua/main.lua"))?;

            let simplified_plan = planner
                .plan()
                .map(|_, weight| weight.id.clone(), |_, weight| weight);
            println!("{:?}", Dot::new(&simplified_plan));

            let mut builder = Builder::new(&mut store, &planner);

            // hold tempdirs until they need to be dropped
            let mut temp_paths = Vec::with_capacity(64);
            let base: &'static Path = Path::new("builds");

            utils::ensure_dir(base)?;
            let output = builder.build(&lua, 2.into(), || {
                temp_paths.push(tempdir_in(base)?);
                let path = temp_paths.last().unwrap().path().to_path_buf();
                Ok(BubblewrapExecutor::new(
                    path,
                    BubblewrapExecutorOptions::default(),
                ))
            });

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

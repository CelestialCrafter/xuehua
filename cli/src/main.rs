pub mod options;

use std::{io::stderr, path::Path};

use eyre::{Context, DefaultHandler, Result};
use log::LevelFilter;
use mlua::Lua;
use petgraph::{dot::Dot, graph::NodeIndex};
use xh_engine::{
    builder::{Builder, BuilderOptions},
    executor::{BubblewrapExecutor, Manager, bubblewrap::BubblewrapExecutorOptions},
    logger,
    package::manifest::Manifest,
    planner::Planner,
    store::LocalStore,
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

            // register apis
            logger::register_module(&lua)?;
            utils::register_module(&lua)?;

            // setup engine modules
            let store_dir = Path::new("store");
            utils::ensure_dir(store_dir)?;
            let mut store = LocalStore::new(store_dir)?;

            let mut planner = Planner::new(&lua);
            planner.run(&lua, Path::new("xuehua/main.lua"))?;
            println!("{:?}", Dot::new(planner.plan()));
            let plan = planner.plan();

            let manifest = Manifest::create(plan, &store)?;

            let mut manager = Manager::default();
            manager.register("runner".to_string(), |env| {
                let executor = BubblewrapExecutor::new(env, BubblewrapExecutorOptions::default())?;
                Ok(Box::new(executor))
            });

            let build_dir = Path::new("builds");
            utils::ensure_dir(build_dir)?;

            let mut builder = Builder::new(
                &mut store,
                &manifest,
                plan,
                manager,
                BuilderOptions {
                    build_dir: build_dir.to_path_buf(),
                },
            );

            // run build
            let runtime = builder.build(&lua, NodeIndex::from(3))?;
            dbg!(runtime);
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

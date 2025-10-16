pub mod options;

use std::{fs::read_dir, io::stderr, path::Path};

use eyre::{Context, DefaultHandler, Result};
use log::{info, LevelFilter};
use mlua::Lua;
use petgraph::graph::NodeIndex;
use xh_engine::{
    builder::{Builder, BuilderOptions},
    executor::{BubblewrapExecutor, Manager, bubblewrap::BubblewrapExecutorOptions},
    logger,
    planner::Planner,
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

            // create engine modules
            let mut planner = Planner::new(&lua);
            planner.run(&lua, Path::new("xuehua/main.lua"))?;

            let mut manager = Manager::default();
            manager.register("runner".to_string(), |env| {
                Ok(Box::new(BubblewrapExecutor::new(
                    env.to_path_buf(),
                    BubblewrapExecutorOptions::default(),
                )?))
            });

            let build_dir = Path::new("builds");
            utils::ensure_dir(build_dir)?;

            let mut builder = Builder::new(
                NodeIndex::from(3),
                &planner,
                &manager,
                BuilderOptions {
                    build_dir: build_dir.to_path_buf(),
                },
            );

            // build target package
            while let Some(result) = builder.next() {
                let (pkg, idx) = result?;
                let content: Vec<_> = read_dir(builder.environment(idx).join("output/wawa"))?.collect();
                info!("package {} was built with contents {:?}", pkg.id, content);
            }
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

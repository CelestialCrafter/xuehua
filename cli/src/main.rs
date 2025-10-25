pub mod options;

use std::{fs, io::stderr, path::Path};

use eyre::{Context, DefaultHandler, Result};

use fern::colors::{Color, ColoredLevelConfig};
use log::{LevelFilter, info, warn};

use mlua::Lua;
use petgraph::{dot::Dot, graph::NodeIndex};
use tokio::runtime::Runtime;
use xh_engine::{
    builder::{Builder, BuilderOptions},
    executor::{
        Manager,
        bubblewrap::{BubblewrapExecutor, BubblewrapExecutorOptions},
    },
    logger,
    planner,
    utils,
};

use crate::options::{Subcommand, get_options};

fn main() -> Result<()> {
    eyre::set_hook(Box::new(DefaultHandler::default_with))
        .wrap_err("error installing eyre handler")?;

    let fern_colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .debug(Color::Blue)
        .warn(Color::Yellow)
        .error(Color::Red);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "({}) {} {}",
                fern_colors.color(record.level()).to_string().to_lowercase(),
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

            // run planner
            let mut planner = planner::Planner::new();
            let chunk = lua.load(fs::read("xuehua/main.lua")?);
            lua.scope(|scope| {
                lua.register_module(
                    planner::MODULE_NAME,
                    scope.create_userdata_ref_mut(&mut planner)?,
                )?;
                scope.add_destructor(|| {
                    if let Err(err) = lua.unload_module(planner::MODULE_NAME) {
                        warn!("could not unload {}: {}", planner::MODULE_NAME, err);
                    }
                });

                chunk.exec()
            })?;
            info!(
                "{:?}",
                Dot::new(&planner.plan().map(|_, w| w.id.to_string(), |_, w| *w))
            );

            // run builder
            let mut manager = Manager::default();
            manager.register("runner".to_string(), |env| {
                Box::new(BubblewrapExecutor::new(
                    env.to_path_buf(),
                    BubblewrapExecutorOptions::default(),
                ))
            });

            let build_root = Path::new("builds");
            utils::ensure_dir(build_root)?;

            let mut builder = Builder::new(
                planner,
                manager,
                lua,
                BuilderOptions {
                    concurrent: 8,
                    root: build_root.to_path_buf(),
                },
            );

            Runtime::new()
                .unwrap()
                .block_on(builder.build(NodeIndex::from(3)));
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

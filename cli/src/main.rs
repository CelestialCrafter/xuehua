pub mod options;

use std::{fs, io::stderr, path::Path, sync::mpsc};

use eyre::{Context, DefaultHandler, Result};

use fern::colors::{Color, ColoredLevelConfig};
use log::{LevelFilter, warn};

use mlua::Lua;
use petgraph::{dot, graph::NodeIndex};
use tokio::runtime::Runtime;
use xh_engine::{
    builder::{Builder, BuilderOptions},
    executor::bubblewrap::{BubblewrapExecutor, BubblewrapExecutorOptions},
    logger, planner, utils,
};

use crate::options::{Action, InspectAction, OPTIONS, ProjectFormat};

fn main() -> Result<()> {
    eyre::set_hook(Box::new(DefaultHandler::default_with))
        .wrap_err("error installing eyre handler")?;

    let colors = ColoredLevelConfig::new()
        .info(Color::Blue)
        .debug(Color::Magenta)
        .trace(Color::BrightBlack)
        .warn(Color::Yellow)
        .error(Color::Red);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "({}) {} {}",
                colors.color(record.level()).to_string().to_lowercase(),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::Debug)
        .chain(stderr())
        .apply()
        .wrap_err("error installing logger")?;

    match &OPTIONS.cli.action {
        Action::Build { packages, .. } => {
            let (lua, planner) = basic_lua_plan(Path::new("xuehua/main.lua"))?;

            // run builder
            let runtime = Runtime::new()?;
            let build_root = Path::new("builds");
            utils::ensure_dir(build_root)?;

            let mut builder = Builder::new(
                planner,
                lua,
                BuilderOptions {
                    concurrent: 12,
                    root: build_root.to_path_buf(),
                },
            );

            builder.with_executor("runner".to_string(), |env| {
                BubblewrapExecutor::new(env, BubblewrapExecutorOptions::default())
            });

            let (results_tx, results_rx) = mpsc::channel();
            let handle = runtime.spawn(async move {
                while let Ok(result) = results_rx.recv() {
                    warn!("build result streamed: {result:?}");
                }
            });

            runtime.block_on(async move {
                // TODO: add resolver api
                for i in 0..4 {
                    builder.build(NodeIndex::from(i), results_tx.clone()).await;
                }

                handle.await
            })?;
        }
        Action::Link { .. } => todo!("link action not implemented"),
        Action::Inspect(action) => match action {
            InspectAction::Project { format } => {
                let (_, planner) = basic_lua_plan(&OPTIONS.cli.project)?;

                match format {
                    ProjectFormat::Dot => println!(
                        "{:?}",
                        dot::Dot::with_attr_getters(
                            &planner.plan(),
                            &[dot::Config::EdgeNoLabel, dot::Config::NodeNoLabel],
                            &|_, linktime| format!(r#"label="{}""#, linktime.weight()),
                            &|_, (_, pkg)| format!(r#"label="{}""#, pkg.id),
                        )
                    ),
                    ProjectFormat::Json => todo!("json format not yet implemented"),
                };
            }
            InspectAction::Packages { .. } => {
                todo!("package inspect not yet implemented")
            }
        },
    }

    Ok(())
}

fn basic_lua_plan(location: &Path) -> Result<(mlua::Lua, planner::Planner)> {
    // FIX: restrict stdlibs
    let lua = Lua::new();

    // register apis
    logger::register_module(&lua)?;
    utils::register_module(&lua)?;

    // run planner
    let mut planner = planner::Planner::new();
    let chunk = lua.load(fs::read(location)?);
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

    Ok((lua, planner))
}

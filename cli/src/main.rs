pub mod options;

use std::{fs, io::stderr, path::Path, sync::mpsc};

use eyre::{Context, DefaultHandler, Result};

use fern::colors::{Color, ColoredLevelConfig};
use jiff::Timestamp;
use log::{LevelFilter, warn};
use mlua::Lua;
use petgraph::dot;
use tokio::task;
use xh_engine::{
    builder::Builder,
    executor::{
        bubblewrap::{BubblewrapExecutor, BubblewrapExecutorOptions},
        http::HttpExecutor,
    },
    logger, planner,
    scheduler::Scheduler,
    utils,
};

use crate::options::{Action, InspectAction, OPTIONS, ProjectFormat};

#[tokio::main]
async fn main() -> Result<()> {
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
                "({}) ({}) {} {}",
                // TODO: color timestamp as trace
                Timestamp::now().strftime("%T"),
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
            let nodes = packages
                .iter()
                .map(|id| {
                    planner
                        .resolve(id)
                        .ok_or_else(|| planner::Error::PackageNotFound(id.clone()))
                })
                .collect::<Result<Vec<_>, planner::Error>>()?;

            // run builder
            let build_root = Path::new("builds");
            utils::ensure_dir(build_root)?;

            let mut scheduler = Scheduler::new(planner.into_inner());
            let builder = Builder::new(Path::new("builds"), &lua)
                .register("bubblewrap".to_string(), 2, |env| {
                    Ok(BubblewrapExecutor::new(
                        env,
                        BubblewrapExecutorOptions::default(),
                    ))
                })
                .register("http".to_string(), 2, |env| Ok(HttpExecutor::new(env)));

            let (results_tx, results_rx) = mpsc::channel();
            let handle = task::spawn(async move {
                while let Ok((id, result)) = results_rx.recv() {
                    warn!("package {id} build result streamed: {result:?}");
                }
            });

            scheduler.schedule(&nodes, &builder, results_tx).await;

            handle.await?
        }
        Action::Link { .. } => todo!("link action not implemented"),
        Action::Inspect(action) => match action {
            InspectAction::Project { format, project } => {
                let (_, planner) = basic_lua_plan(&project)?;

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
    let chunk = lua.load(fs::read(location)?).into_function()?;
    lua.scope(|scope| {
        let environment = chunk
            .environment()
            .ok_or(mlua::Error::external("chunk does not have an environment"))?;
        environment.set("planner", scope.create_userdata_ref_mut(&mut planner)?)?;

        chunk.call::<()>(())
    })?;

    Ok((lua, planner))
}

pub mod options;

use std::{
    fs,
    io::{Write, stderr, stdout},
    path::Path,
    sync::mpsc,
};

use eyre::{Context, DefaultHandler, Result};

use fern::colors::{Color, ColoredLevelConfig};
use jiff::Timestamp;
use log::{LevelFilter, debug, warn};
use mlua::Lua;
use petgraph::{dot, graph::NodeIndex};
use tokio::task;
use xh_engine::{
    builder::Builder,
    executor::{
        bubblewrap::{BubblewrapExecutor, BubblewrapExecutorOptions},
        http::HttpExecutor,
    },
    logger,
    package::PackageId,
    planner::{Error as PlannerError, Planner},
    scheduler::Scheduler,
    utils,
};

use crate::options::{
    OPTIONS, cli::Action, cli::InspectAction, cli::PackageFormat, cli::ProjectFormat,
};

fn resolve_many(
    planner: &Planner,
    packages: &Vec<PackageId>,
) -> Result<Vec<NodeIndex>, PlannerError> {
    packages
        .iter()
        .map(|id| {
            planner
                .resolve(id)
                .ok_or_else(|| PlannerError::PackageNotFound(id.clone()))
        })
        .collect::<Result<Vec<_>, PlannerError>>()
}

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
            let (lua, planner) = populate_lua(&OPTIONS.cli.project)?;
            let nodes = resolve_many(&planner, packages)?;

            // run builder
            let build_root = tempfile::tempdir_in(&OPTIONS.base.build_directory)?;
            debug!("building to {:?}", build_root.path());

            let mut scheduler = Scheduler::new(planner.into_inner());
            let builder = Builder::new(build_root.path(), &lua)
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

            // TODO: push builds into store and delete build dir
            let _ = build_root.keep();

            handle.await?
        }
        Action::Link { .. } => todo!("link action not implemented"),
        Action::Inspect(action) => match action {
            InspectAction::Project { format } => {
                let (_, planner) = populate_lua(&OPTIONS.cli.project)?;

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
            InspectAction::Packages { packages, format } => match format {
                // TODO: styled output instead of "markdown"
                // TODO: output store artifacts for pkg
                PackageFormat::Human => {
                    let (_, planner) = populate_lua(&OPTIONS.cli.project)?;
                    let plan = planner.plan();

                    let mut stdout = stdout().lock();
                    for (i, node) in resolve_many(&planner, packages)?.into_iter().enumerate() {
                        let pkg = &plan[node];
                        writeln!(stdout, "# {}\n", pkg.id)?;

                        for dependency in pkg.dependencies() {
                            let id = plan[dependency.node].id.to_string();
                            let time = dependency.time.to_string();
                            writeln!(stdout, "**Dependency**: {id} at {time}")?;
                        }

                        // .join would be less efficient here
                        if i + 1 != packages.len() {
                            writeln!(stdout, "")?;
                        }
                    }
                }
                PackageFormat::Json => todo!("json format not yet implemented"),
            },
        },
    }

    Ok(())
}

fn populate_lua(location: &Path) -> Result<(mlua::Lua, Planner)> {
    // FIX: restrict stdlibs
    let lua = Lua::new();

    // register apis
    logger::register_module(&lua)?;
    utils::register_module(&lua)?;

    // run planner
    let mut planner = Planner::new();
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

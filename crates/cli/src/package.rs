use std::{
    io::Write,
    path::Path,
    sync::{Arc, mpsc},
};

use crate::options::{
    cli::{PackageAction, ProjectFormat},
    get_opts,
};

use log::info;
use petgraph::{dot, graph::NodeIndex};
use tokio::task;
use xh_backend_lua::LuaBackend;
use xh_engine::{
    backend::Backend,
    builder::Builder,
    name::PackageName,
    planner::{Frozen, Planner},
    scheduler::{Event, Scheduler},
    store::Store,
};
use xh_executor_bubblewrap::{BubblewrapExecutor, Options as BubblewrapOptions};
use xh_executor_http::HttpExecutor;
use xh_reports::{partition_result, prelude::*};
use xh_store_sqlite::SqliteStore;

use crate::options::cli::{InspectAction, PackageFormat};

#[derive(Debug, IntoReport)]
pub enum PackageActionError {
    #[message("could not initialize planner")]
    Initialize,
    #[message("could not execute link action")]
    Link,
    #[message("could not execute inspect action")]
    Inspect,
}

pub async fn handle(project: &Path, action: &PackageAction) -> Result<(), ()> {
    let mut planner = Planner::new();
    LuaBackend::new()
        .and_then(|backend| backend.plan(&mut planner, project))
        .wrap_with(PackageActionError::Initialize)
        .erased()?;

    let planner = planner
        .freeze()
        .wrap_with(PackageActionError::Initialize)
        .erased()?;

    match action {
        PackageAction::Build { packages, .. } => build(&planner, packages).await.erased()?,
        PackageAction::Link { .. } => todo!("link action not implemented"),
        PackageAction::Inspect(action) => match action {
            InspectAction::Project { format } => inspect_project(&planner, format),
            InspectAction::Packages { packages, format } => {
                inspect_packages(planner, packages, format)
                    .wrap_with(PackageActionError::Inspect)
                    .erased()?
            }
        },
    }

    Ok(())
}

fn inspect_packages(
    planner: Planner<Frozen>,
    packages: &Vec<PackageName>,
    format: &PackageFormat,
) -> Result<(), ()> {
    match format {
        // TODO: styled output instead of "markdown"
        // TODO: output store artifacts for pkg
        PackageFormat::Human => {
            let mut stdout = std::io::stdout().lock();
            for (i, node) in resolve_many(&planner, packages)
                .erased()?
                .into_iter()
                .enumerate()
            {
                let plan = planner.graph();
                let pkg = &plan[node];

                writeln!(stdout, "# {}\n", pkg.name).erased()?;
                for dependency in &pkg.dependencies {
                    writeln!(
                        stdout,
                        "**Dependency**: {} at {}",
                        dependency.name, dependency.time
                    )
                    .erased()?;
                }

                // .join would be less efficient here
                if i + 1 != packages.len() {
                    writeln!(stdout, "").erased()?;
                }
            }
        }
        PackageFormat::Json => todo!("json format not yet implemented"),
    }

    Ok(())
}

fn inspect_project(planner: &Planner<Frozen>, format: &ProjectFormat) {
    match format {
        ProjectFormat::Dot => println!(
            "{:?}",
            dot::Dot::with_attr_getters(
                planner.graph(),
                &[dot::Config::EdgeNoLabel, dot::Config::NodeNoLabel],
                &|_, linktime| format!(r#"label="{}""#, linktime.weight()),
                &|_, (_, pkg)| format!(r#"label="{}""#, pkg.name),
            )
        ),
        ProjectFormat::Json => todo!("json format not yet implemented"),
    }
}

#[derive(Default, Debug, IntoReport)]
#[message("could not execute build action")]
struct BuildActionError;

async fn build(
    planner: &Planner<Frozen>,
    packages: &Vec<PackageName>,
) -> StdResult<(), Report<BuildActionError>> {
    let locations = &get_opts().base.locations;
    let nodes = resolve_many(planner, packages).wrap()?;
    let mut store = SqliteStore::new(locations.store.clone()).wrap()?;
    let builder: Arc<_> = Builder::new(locations.build.clone())
        .register(|ctx| Ok(BubblewrapExecutor::new(ctx, BubblewrapOptions::default())))
        .register(|ctx| Ok(HttpExecutor::new(ctx)))
        .into();

    let mut scheduler = Scheduler::new(planner, builder.as_ref());
    let builder = builder.clone();

    let (results_tx, results_rx) = mpsc::channel();
    let handle = task::spawn(async move {
        let mut failures = Vec::new();
        while let Ok(event) = results_rx.recv() {
            match event {
                Event::Started { request } => info!(
                    request:? = request;
                    "started package build"
                ),
                Event::Finished { request, result } => {
                    info!(
                        request:? = request,
                        status = if result.is_ok() { "succeeded" } else { "failed" };
                        "package build finished"
                    );

                    match result {
                        Ok(()) => {
                            let archive = builder
                                .fetch(&request.id)
                                .ok()
                                .flatten()
                                .expect("could not fetch package output");

                            store
                                .register_artifact(archive)
                                .await
                                .expect("could not register artifact");
                        }
                        Err(report) => failures.push(report),
                    };
                }
            };
        }

        failures
    });

    scheduler.schedule(&nodes, results_tx).await;

    let failures = handle.await.wrap()?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(BuildActionError::default()
            .into_report()
            .with_children(failures))
    }
}

#[derive(Debug, IntoReport)]
#[message("could not resolve packages")]
#[context(packages)]
pub struct PackageResolveError {
    packages: Vec<PackageName>,
}

fn resolve_many(
    planner: &Planner<Frozen>,
    packages: &Vec<PackageName>,
) -> Result<Vec<NodeIndex>, PackageResolveError> {
    let result = partition_result(
        packages
            .iter()
            .map(|name| planner.resolve(name).ok_or_else(|| name.clone())),
    );

    match result {
        Ok(nodes) => Ok(nodes),
        Err(packages) => Err(PackageResolveError { packages }.into()),
    }
}

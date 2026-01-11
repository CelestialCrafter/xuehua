use std::{
    io::Write,
    path::Path,
    sync::{Arc, mpsc},
};

use crate::options::{
    cli::{PackageAction, ProjectFormat},
    get_opts,
};

use eyre::OptionExt;
use log::info;
use petgraph::{dot, graph::NodeIndex};
use tokio::task;
use xh_backend_lua::LuaBackend;
use xh_engine::{
    backend::Backend,
    builder::Builder,
    package::PackageName,
    planner::{Frozen, Planner},
    scheduler::{Event, Scheduler},
    store::Store,
};
use xh_executor_bubblewrap::{BubblewrapExecutor, Options as BubblewrapOptions};
use xh_executor_http::HttpExecutor;
use xh_store_local::LocalStore;

use crate::options::cli::{InspectAction, PackageFormat};

pub async fn handle(project: &Path, action: &PackageAction) -> Result<(), eyre::Error> {
    let backend = Arc::new(LuaBackend::new()?);
    let mut planner = Planner::new();
    backend.plan(&mut planner, project)?;
    let planner = planner.freeze(backend.as_ref())?;

    match action {
        PackageAction::Build { packages, .. } => {
            let locations = &get_opts().base.locations;
            let nodes =
                resolve_many(&planner, packages).ok_or_eyre("could not resolve all package ids")?;

            let mut store = LocalStore::new(locations.store.clone())?;
            let builder: Arc<_> = Builder::new(locations.build.clone(), backend.clone())
                .register(|ctx| Ok(BubblewrapExecutor::new(ctx, BubblewrapOptions::default())))
                .register(|ctx| Ok(HttpExecutor::new(ctx)))
                .into();

            let mut scheduler = Scheduler::new(&planner, builder.as_ref());

            let builder = builder.clone();
            let (results_tx, results_rx) = mpsc::channel();
            let handle = task::spawn(async move {
                while let Ok(event) = results_rx.recv() {
                    info!("build result streamed: {:?}", event);

                    if let Event::Finished {
                        request,
                        result: Ok(_),
                    } = event
                    {
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
                }
            });

            scheduler.schedule(&nodes, results_tx).await;

            handle.await?
        }
        PackageAction::Link { .. } => todo!("link action not implemented"),
        PackageAction::Inspect(action) => match action {
            InspectAction::Project { format } => {
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
                };
            }
            InspectAction::Packages { packages, format } => match format {
                // TODO: styled output instead of "markdown"
                // TODO: output store artifacts for pkg
                PackageFormat::Human => {
                    let mut stdout = std::io::stdout().lock();
                    for (i, node) in resolve_many(&planner, packages)
                        .ok_or_eyre("could not resolve all package ids")?
                        .into_iter()
                        .enumerate()
                    {
                        let plan = planner.graph();
                        let pkg = &plan[node];

                        writeln!(stdout, "# {}\n", pkg.name)?;
                        for dependency in &pkg.dependencies {
                            writeln!(
                                stdout,
                                "**Dependency**: {} at {}",
                                plan[dependency.node].name, dependency.time
                            )?;
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

fn resolve_many<B: Backend>(
    planner: &Planner<Frozen<'_, B>>,
    packages: &Vec<PackageName>,
) -> Option<Vec<NodeIndex>> {
    packages
        .iter()
        .map(|id| planner.resolve(id))
        .collect::<Option<Vec<_>>>()
}

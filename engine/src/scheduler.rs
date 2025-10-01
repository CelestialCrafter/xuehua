use std::{mem, sync::mpsc};

use futures_util::{StreamExt, stream::FuturesUnordered};
use log::{debug, trace};
use petgraph::{
    Direction,
    graph::{DiGraph, NodeIndex},
    visit::{Dfs, EdgeRef},
};

use crate::{
    builder::{BuildInfo, Builder, Error},
    package::{Package, PackageId},
    planner::{LinkTime, Plan},
    utils::passthru::PassthruHashSet,
};

#[derive(Debug)]
enum PackageState {
    Unbuilt {
        package: Package,
        remaining: usize,
    },
    // NOTE: Scheduler can be put into a non-ideal state if Builder::build() panics
    Building,
    Built {
        package: Package,
        runtime: Vec<NodeIndex>,
    },
}

#[derive(Debug)]
pub enum Event {
    Started,
    Finished(Result<(), Error>),
}

/// Package build scheduler
///
/// The builder traverses through a [`Planner`]'s instructions and queues builds of the packages needed to build the target package
pub struct Scheduler {
    state: DiGraph<PackageState, LinkTime>,
}

impl Scheduler {
    pub fn new(plan: Plan) -> Self {
        let mut state = plan.into_inner().map_owned(
            |_, weight| PackageState::Unbuilt {
                remaining: 0,
                package: weight.into(),
            },
            |_, weight| weight,
        );

        for node in state.node_indices() {
            let count = state.neighbors_directed(node, Direction::Outgoing).count();
            match state[node] {
                PackageState::Unbuilt {
                    ref mut remaining, ..
                } => *remaining = count,
                _ => unreachable!(),
            }
        }

        Self { state }
    }

    pub async fn schedule(
        &mut self,
        targets: &[NodeIndex],
        builder: &Builder<'_>,
        events: mpsc::Sender<(PackageId, Event)>,
    ) {
        let mut futures = FuturesUnordered::new();
        let mut subset = PassthruHashSet::default();
        let run_builder = async |info: BuildInfo| {
            let _ = events.send((info.package.id.clone(), Event::Started));
            let result = builder.build(&info).await;
            (info, result)
        };

        // compute subset and build leaf packages
        let mut visitor = Dfs::empty(&self.state);
        for target in targets {
            visitor.move_to(*target);
            while let Some(node) = visitor.next(&self.state) {
                subset.insert(node);
                if let Some(info) = self.prepare_info(node) {
                    trace!("adding package {} as a leaf", info.package.id);
                    futures.push(run_builder(info));
                }
            }
        }

        // main build loop
        while let Some((
            BuildInfo {
                node: target,
                package,
                runtime,
                ..
            },
            result,
        )) = futures.next().await
        {
            let errored = result.is_err();
            let _ = events.send((package.id.clone(), Event::Finished(result)));

            if errored {
                self.state[target] = PackageState::Unbuilt {
                    package,
                    remaining: 0,
                };
                continue;
            } else {
                self.state[target] = PackageState::Built { runtime, package };
            };

            for parent in self
                .state
                .neighbors_directed(target, Direction::Incoming)
                .collect::<Vec<_>>()
            {
                match &mut self.state[parent] {
                    PackageState::Unbuilt { remaining, package } => {
                        *remaining -= 1;
                        debug!("{} has {} dependencies remaining", package.id, remaining);
                    }
                    state => panic!(
                        "parent node {parent:?} should not be in the {state:?} state before child node {target:?} finishes building"
                    ),
                }

                if subset.contains(&parent) {
                    if let Some(info) = self.prepare_info(parent) {
                        futures.push(run_builder(info));
                    }
                }
            }
        }
    }

    fn prepare_info(&mut self, target: NodeIndex) -> Option<BuildInfo> {
        // check if package can be built
        match self.state[target] {
            PackageState::Unbuilt {
                ref mut remaining, ..
            } if *remaining == 0 => (),
            _ => return None,
        };

        let package = match mem::replace(&mut self.state[target], PackageState::Building) {
            PackageState::Unbuilt { package, .. } => package,
            _ => unreachable!(),
        };

        // gather dependencies into the build closure
        let mut buildtime = Vec::default();
        let mut runtime = Vec::default();
        for edge in self.state.edges_directed(target, Direction::Outgoing) {
            let child = edge.target();
            match &self.state[child] {
                PackageState::Built {
                    runtime: dep_runtime,
                    ..
                } => {
                    let closure = match edge.weight() {
                        LinkTime::Runtime => &mut runtime,
                        LinkTime::Buildtime => &mut buildtime,
                    };

                    closure.extend(dep_runtime.into_iter());
                    closure.push(child);
                }
                state => panic!(
                    "child node {child:?} should not be in the {state:?} while building {target:?}"
                ),
            }
        }

        Some(BuildInfo {
            node: target,
            package,
            runtime,
            buildtime,
        })
    }
}

use std::sync::mpsc;

use futures_util::{StreamExt, stream::FuturesUnordered};
use log::{debug, trace};
use petgraph::{Direction, graph::NodeIndex, visit::Dfs};

use crate::{
    backend::Backend,
    builder::{BuildRequest, Builder, Dispatch, Error as BuilderError, Initialize},
    planner::{Frozen, Planner},
    utils::passthru::{PassthruHashMap, PassthruHashSet},
};

#[derive(Debug)]
enum PackageState {
    Unbuilt { remaining: usize },
    Built,
}

// TODO: add the ability for packages to report custom statuses
#[derive(Debug)]
pub enum Event<B: Backend> {
    Started {
        request: BuildRequest,
    },
    Finished {
        request: BuildRequest,
        result: Result<(), BuilderError<B>>,
    },
}

/// Package build scheduler
///
/// The builder traverses through a [`Planner`]'s instructions and queues builds of the packages needed to build the target package
pub struct Scheduler<'a, B: Backend, E> {
    state: PassthruHashMap<NodeIndex, PackageState>,
    planner: &'a Planner<Frozen<'a, B>>,
    builder: &'a Builder<B, E>,
}

impl<'a, B, E> Scheduler<'a, B, E>
where
    B: Backend,
    E: Initialize,
    E::Output: Dispatch<B>
{
    pub fn new(planner: &'a Planner<Frozen<B>>, builder: &'a Builder<B, E>) -> Self {
        let plan = planner.graph();
        let state = plan
            .node_indices()
            .map(|node| {
                (
                    node,
                    PackageState::Unbuilt {
                        remaining: plan.neighbors_directed(node, Direction::Outgoing).count(),
                    },
                )
            })
            .collect();

        Self {
            planner,
            builder,
            state,
        }
    }

    pub async fn schedule(&mut self, targets: &[NodeIndex], events: mpsc::Sender<Event<B>>) {
        let mut futures = FuturesUnordered::new();
        let plan = self.planner.graph();

        let build = async |node: NodeIndex| {
            let request = BuildRequest {
                id: fastrand::u64(..),
                target: node,
            };

            (request, self.builder.build(self.planner, request).await)
        };

        // compute subset and build leaf packages
        let mut subset = PassthruHashSet::default();
        let mut visitor = Dfs::empty(&plan);
        for target in targets {
            visitor.move_to(*target);
            while let Some(node) = visitor.next(plan) {
                subset.insert(node);
                if let PackageState::Unbuilt { remaining: 0, .. } = self.state[&target] {
                    trace!("adding node {:?} as a leaf", node);
                    futures.push(build(node));
                }
            }
        }

        // main build loop
        while let Some((request, result)) = futures.next().await {
            let errored = result.is_err();
            let _ = events.send(Event::Finished { request, result });

            if errored {
                continue;
            }

            self.state.insert(request.target, PackageState::Built);
            for parent in plan.neighbors_directed(request.target, Direction::Incoming) {
                let Some(PackageState::Unbuilt { remaining }) = self.state.get_mut(&parent) else {
                    unreachable!(
                        "parent node {parent:?} should be unbuilt state while child node {:?} is building",
                        request.target
                    );
                };

                *remaining -= 1;
                debug!("{:?} has {} dependencies remaining", parent, remaining);
                if *remaining == 0 && subset.contains(&parent) {
                    futures.push(build(parent));
                }
            }
        }
    }
}

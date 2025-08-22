pub mod config;

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use log::trace;
use petgraph::{
    Direction,
    acyclic::Acyclic,
    data::{Build, DataMapMut},
    graph::{DiGraph, NodeIndex},
    visit::{Dfs, EdgeRef},
};
use smol_str::SmolStr;
use xh_reports::prelude::*;

use crate::{
    package::{LinkTime, Package, PackageName},
    utils::passthru::PassthruHashSet,
};

#[derive(Debug, IntoReport)]
#[message("package has conflicting definitions")]
#[suggestion("rename {package} to something different")]
#[context(package)]
pub struct ConflictError {
    #[format(suggestion)]
    pub package: PackageName,
}

#[derive(Debug, IntoReport)]
#[message("package dependencies form a cycle")]
#[suggestion("remove the dependency creating a cycle")]
#[context(from, to)]
pub struct CycleError {
    from: PackageName,
    to: PackageName,
}

#[derive(Debug, IntoReport)]
#[message("unregistered dependency on {package}")]
#[suggestion("register the dependency as a package")]
#[context(package)]
pub struct UnregisteredDependency {
    package: PackageName,
}

#[derive(Default, Debug, IntoReport)]
#[message("could not evaluate plan")]
pub struct Error;

#[derive(Clone, Default, Debug)]
pub struct NamespaceTracker(Arc<RwLock<Vec<SmolStr>>>);

impl NamespaceTracker {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn current(&self) -> Vec<SmolStr> {
        self.0.read().unwrap().clone()
    }

    #[inline]
    pub fn scope<R, S: Into<SmolStr>>(&self, segment: S, func: impl FnOnce() -> R) -> R {
        let get = || self.0.write().unwrap();
        get().push(segment.into());
        let rval = func();
        get().pop();
        rval
    }
}

#[derive(Default, Debug, Clone)]
pub struct DependencyClosure {
    runtime: PassthruHashSet<NodeIndex>,
    buildtime: PassthruHashSet<NodeIndex>,
}

pub type Plan = Acyclic<DiGraph<Package, LinkTime>>;
pub type PackageId = blake3::Hash;

pub struct Frozen;
pub struct Unfrozen;

#[derive(Debug)]
pub struct Planner<State> {
    graph: Plan,
    packages: HashMap<PackageName, NodeIndex>,
    _marker: PhantomData<State>,
}

impl Planner<Unfrozen> {
    #[inline]
    pub fn new() -> Self {
        Self {
            graph: Default::default(),
            packages: Default::default(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn freeze(self) -> Result<Planner<Frozen>, Error> {
        Planner::<Frozen>::new(self)
    }

    pub fn register(&mut self, package: Package) -> Result<NodeIndex, Error> {
        trace!("registering package {}", package.name);

        if self.packages.contains_key(&package.name) {
            return Err(ConflictError {
                package: package.name,
            }
            .wrap());
        }

        let name = package.name.clone();
        let node = self.graph.add_node(package);
        self.packages.insert(name, node);

        Ok(node)
    }
}

impl Planner<Frozen> {
    fn new(unfrozen: Planner<Unfrozen>) -> Result<Self, Error> {
        let mut planner = Planner {
            graph: unfrozen.graph,
            packages: unfrozen.packages,
            _marker: PhantomData,
        };

        // .collect so we don't hold a reference to the graph
        let order = planner.graph.nodes_iter().collect::<Vec<_>>();
        for node in order {
            // take dependencies so we don't hold a reference to the graph
            let dependencies = std::mem::take(
                &mut planner
                    .graph
                    .node_weight_mut(node)
                    .expect("node should exist")
                    .dependencies,
            );

            for dependency in dependencies {
                planner
                    .graph
                    .try_add_edge(
                        node,
                        planner
                            .resolve(&dependency.name)
                            .ok_or_else(|| UnregisteredDependency {
                                package: dependency.name.clone(),
                            })
                            .wrap()?,
                        dependency.time,
                    )
                    .map_err(|_| CycleError {
                        from: planner.graph[node].name.clone(),
                        to: dependency.name.clone(),
                    })
                    .wrap()?;
            }
        }

        Ok(planner)
    }

    #[inline]
    pub fn graph(&self) -> &Plan {
        &self.graph
    }

    // TODO: cache closure
    pub fn closure(&self, node: NodeIndex) -> Option<DependencyClosure> {
        let compute_closure = |dependencies: Vec<(NodeIndex, LinkTime)>| {
            let mut runtime = PassthruHashSet::default();
            let mut visitor = Dfs::empty(&self.graph);

            for (node, _) in dependencies {
                visitor.move_to(node);
                while let Some(node) = visitor.next(&self.graph) {
                    runtime.extend(
                        self.graph
                            .edges_directed(node, Direction::Outgoing)
                            .filter(|edge| *edge.weight() == LinkTime::Runtime)
                            .map(|edge| edge.target()),
                    );
                }
            }

            runtime
        };

        let (runtime, buildtime) = self
            .graph
            .edges_directed(node, Direction::Outgoing)
            .map(|edge| (edge.target(), *edge.weight()))
            .partition(|(_, time)| *time == LinkTime::Runtime);

        Some(DependencyClosure {
            runtime: compute_closure(runtime),
            buildtime: compute_closure(buildtime),
        })
    }

    // TODO: cache identity
    pub fn identity(&self, node: NodeIndex) -> Option<PackageId> {
        let mut hasher = blake3::Hasher::new();
        let mut hash_pkg = |pkg: &Package| {
            hasher.update(pkg.name.identifier.as_bytes());
            for segment in &pkg.name.namespace {
                hasher.update(segment.as_bytes());
            }

            for request in &pkg.requests {
                hasher.update(request.executor.as_bytes());

                let mut payload_hasher = std::hash::DefaultHasher::new();
                request.payload.hash(&mut payload_hasher);
                hasher.update(&payload_hasher.finish().to_le_bytes());
            }
        };

        let closure = self.closure(node)?;
        std::iter::once(&node)
            .chain(closure.runtime.iter())
            .chain(closure.buildtime.iter())
            .for_each(|node| hash_pkg(&self.graph[*node]));

        Some(hasher.finalize())
    }

    #[inline]
    pub fn resolve(&self, id: &PackageName) -> Option<NodeIndex> {
        self.packages.get(id).copied()
    }
}

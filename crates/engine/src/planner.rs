pub mod config;

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use log::trace;
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DiGraph, NodeIndex},
    visit::Dfs,
};
use smol_str::SmolStr;
use xh_reports::prelude::*;

use crate::{
    package::{Dependency, LinkTime, Package, PackageName},
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

#[derive(Default, Debug)]
pub struct Planner {
    graph: Plan,
    packages: HashMap<PackageName, NodeIndex>,
}

impl Planner {
    #[inline]
    pub fn new() -> Self {
        Self::default()
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
        let dependencies = package.dependencies.clone();
        let node = self.graph.add_node(package);

        for dependency in dependencies.clone() {
            self.graph
                .try_add_edge(node, dependency.node, dependency.time)
                .map_err(|_| {
                    CycleError {
                        from: name.clone(),
                        to: self.graph[dependency.node].name.clone(),
                    }
                    .wrap()
                })?;
        }

        self.packages.insert(name, node);
        Ok(node)
    }

    #[inline]
    pub fn graph(&self) -> &Plan {
        &self.graph
    }

    #[inline]
    pub fn resolve(&self, id: &PackageName) -> Option<NodeIndex> {
        self.packages.get(id).copied()
    }

    // TODO: cache closure
    pub fn closure(&self, node: NodeIndex) -> Option<DependencyClosure> {
        let compute_closure = |dependencies: Vec<Dependency>| {
            let mut runtime = PassthruHashSet::default();
            let mut visitor = Dfs::empty(&self.graph);

            for node in dependencies.into_iter().map(|d| d.node) {
                visitor.move_to(node);
                while let Some(node) = visitor.next(&self.graph) {
                    runtime.extend(
                        self.graph[node]
                            .dependencies
                            .iter()
                            .filter_map(|d| (d.time == LinkTime::Runtime).then_some(d.node)),
                    );
                }
            }

            runtime
        };

        let (runtime, buildtime) = self.graph[node]
            .dependencies
            .iter()
            .partition(|dependency| dependency.time == LinkTime::Runtime);

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
}

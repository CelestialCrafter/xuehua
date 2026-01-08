use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use derivative::Derivative;
use log::trace;
use petgraph::{
    acyclic::Acyclic,
    data::{Build, DataMapMut},
    graph::{DiGraph, NodeIndex},
    visit::Dfs,
};
use smol_str::SmolStr;
use thiserror::Error;

use crate::{
    backend::Backend,
    package::{Dependency, LinkTime, Package, PackageName},
    utils::passthru::PassthruHashSet,
};

#[derive(Error, Debug)]
pub enum Error<B: Backend> {
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageName },
    #[error("cycle detected from package {from:?} to package {to:?}")]
    Cycle { from: PackageName, to: PackageName },
    #[error(transparent)]
    BackendError(B::Error),
}

#[derive(Clone, Default, Debug)]
pub struct NamespaceTracker(Arc<RwLock<Vec<SmolStr>>>);

impl NamespaceTracker {
    #[inline(always)]
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

#[derive(Derivative)]
#[derivative(Debug, Clone(bound = ""))]
pub struct Config<B: Backend> {
    name: PackageName,
    pub current: B::Value,
    #[derivative(Debug = "ignore")]
    pub apply: Arc<dyn Fn(B::Value) -> Result<Package<B>, B::Error> + Send + Sync>,
}

impl<B: Backend> Config<B> {
    #[inline(always)]
    pub fn new<F>(identifier: impl Into<SmolStr>, defaults: B::Value, apply: F) -> Self
    where
        F: Fn(B::Value) -> Result<Package<B>, B::Error>,
        F: Send + Sync + 'static,
    {
        Config {
            name: PackageName {
                identifier: identifier.into(),
                namespace: Default::default(),
            },
            current: defaults,
            apply: Arc::new(apply),
        }
    }
}

pub type Plan<B> = Acyclic<DiGraph<Package<B>, LinkTime>>;

pub type PackageId = blake3::Hash;

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct Unfrozen<B: Backend> {
    configs: Vec<Config<B>>,
    namespace: NamespaceTracker,
}

#[derive(Debug)]
pub struct Frozen<'a, B: Backend> {
    plan: Plan<B>,
    backend: &'a B,
}

/// Package dependency graph generator
///
/// The planner executes the lua source code then generates a DAG of packages and their dependencies.
///
/// # Examples
///
/// ```lua
/// local plan = require("xuehua.planner")
/// local utils = require("xuehua.utils")
///
/// local package_2 = plan.package {
///   id = "package-2",
///   dependencies = {},
///   metadata = {},
///   build = function() end
/// }
///
/// plan.package {
///   id = "package-1",
///   dependencies = { utils.runtime(package_2) },
///   metadata = {},
///   build = function() end
/// }
/// ```
///
/// ```rust
/// use std::path::Path;
/// use petgraph::dot::Dot;
/// use mlua::Lua;
/// use xh_engine::{utils, planner::Planner};
///
/// let lua = Lua::new();
/// utils::inject(&lua)?;
///
/// let mut planner = Planner::new();
/// planner.run(&lua, Path::new("plan.lua"))?;
///
/// let simplified_plan = planner
///     .plan()
///     .map(|_, weight| &weight.id, |_, weight| weight);
///
/// println!("{:?}", Dot::new(&simplified_plan));
/// // digraph {
/// //     0 [ label = "\"package-2\"" ]
/// //     1 [ label = "\"package-1\"" ]
/// //     1 -> 0 [ label = "Runtime" ]
/// // }
///
/// # Ok::<_, xh_engine::planner::Error>(())
/// ```
#[derive(Debug)]
pub struct Planner<State> {
    state: State,
    registered: HashMap<PackageName, NodeIndex>,
}

impl<B: Backend> Default for Planner<Unfrozen<B>> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            registered: Default::default(),
        }
    }
}

impl<B: Backend> Planner<Unfrozen<B>> {
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline(always)]
    pub fn freeze(self, backend: &B) -> Result<Planner<Frozen<'_, B>>, Error<B>> {
        Planner::<Frozen<B>>::new(self, backend)
    }

    #[inline(always)]
    pub fn namespace(&self) -> NamespaceTracker {
        self.state.namespace.clone()
    }

    #[inline(always)]
    fn add_config(&mut self, config: Config<B>) -> NodeIndex {
        let node = NodeIndex::new(self.state.configs.len());
        self.state.configs.push(config);
        node
    }

    pub fn configure(
        &mut self,
        source: NodeIndex,
        identifier: SmolStr,
        modify: impl FnOnce(B::Value) -> Result<B::Value, B::Error>,
    ) -> Option<Result<NodeIndex, Error<B>>> {
        let name = PackageName {
            identifier,
            namespace: self.state.namespace.current(),
        };

        trace!("configuring from {source:?} into {name}");

        self.state
            .configs
            .get(source.index())
            .cloned()
            .map(|source| {
                let config = Config {
                    current: modify(source.current).map_err(Error::BackendError)?,
                    apply: source.apply,
                    name,
                };

                Ok(self.add_config(config))
            })
    }

    pub fn register(&mut self, mut config: Config<B>) -> Result<NodeIndex, Error<B>> {
        trace!("registering config {}", config.name);

        config.name.namespace = self.state.namespace.current();
        if self.registered.contains_key(&config.name) {
            return Err(Error::Conflict {
                package: config.name,
            });
        }

        let name = config.name.clone();
        let node = self.add_config(config);
        self.registered.insert(name, node);

        Ok(node)
    }
}

impl<'a, B: Backend> Planner<Frozen<'a, B>> {
    fn new(unfrozen: Planner<Unfrozen<B>>, backend: &'a B) -> Result<Self, Error<B>> {
        let mut plan: Plan<_> = Plan::new();

        for config in unfrozen.state.configs.into_iter() {
            let mut pkg = (config.apply)(config.current).map_err(Error::BackendError)?;
            pkg.name = config.name;

            plan.add_node(pkg);
        }

        for node in plan.node_indices() {
            let dependencies = std::mem::take(
                &mut plan
                    .node_weight_mut(node)
                    .expect("node should be in graph")
                    .dependencies,
            );

            for dependency in dependencies {
                plan.try_add_edge(node, dependency.node, dependency.time)
                    .map_err(|_| Error::Cycle {
                        from: plan[node].name.clone(),
                        to: plan[dependency.node].name.clone(),
                    })?;
            }
        }

        Ok(Self {
            state: Frozen { plan, backend },
            registered: unfrozen.registered,
        })
    }

    #[inline(always)]
    pub fn graph(&self) -> &Plan<B> {
        &self.state.plan
    }

    // TODO: cache closure
    pub fn closure(&self, node: NodeIndex) -> Option<DependencyClosure> {
        let compute_closure = |dependencies: Vec<Dependency>| {
            let mut runtime = PassthruHashSet::default();
            let mut visitor = Dfs::empty(&self.state.plan);

            for node in dependencies.into_iter().map(|d| d.node) {
                visitor.move_to(node);
                while let Some(node) = visitor.next(&self.state.plan) {
                    runtime.extend(
                        self.state.plan[node]
                            .dependencies
                            .iter()
                            .filter_map(|d| (d.time == LinkTime::Runtime).then_some(d.node)),
                    );
                }
            }

            runtime
        };

        let (runtime, buildtime) = self.state.plan[node]
            .dependencies
            .iter()
            .partition(|dependency| dependency.time == LinkTime::Runtime);

        Some(DependencyClosure {
            runtime: compute_closure(runtime),
            buildtime: compute_closure(buildtime),
        })
    }

    // TODO: cache identity
    pub fn identity(&self, node: NodeIndex) -> Option<Result<PackageId, B::Error>> {
        let mut hasher = blake3::Hasher::new();
        let mut hash_pkg = |pkg: &Package<B>| {
            hasher.update(pkg.name.identifier.as_bytes());
            for segment in &pkg.name.namespace {
                hasher.update(segment.as_bytes());
            }

            for request in &pkg.requests {
                hasher.update(request.executor.as_bytes());
                self.state.backend.hash(&mut hasher, &request.payload)?;
            }

            Ok(())
        };

        let closure = self.closure(node)?;
        let result = std::iter::once(&node)
            .chain(closure.runtime.iter())
            .chain(closure.buildtime.iter())
            .try_for_each(|node| hash_pkg(&self.state.plan[*node]));

        Some(match result {
            Ok(()) => Ok(hasher.finalize()),
            Err(err) => Err(err),
        })
    }
}

impl<State> Planner<State> {
    #[inline(always)]
    pub fn resolve(&self, id: &PackageName) -> Option<NodeIndex> {
        self.registered.get(id).copied()
    }
}

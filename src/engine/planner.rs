use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::Path,
};

use log::warn;
use mlua::{ExternalResult, Lua};
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DiGraph, NodeIndex},
};
use thiserror::Error;

use crate::{
    engine::package::{DependencyType, Package, PackageId},
    utils::LuaError,
};

#[derive(Error, Debug)]
pub enum PlannerError {
    #[error("package {package} not found")]
    NotFound { package: PackageId },
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageId },
    #[error("cycle detected from {from} to {to}")]
    Cyclic { from: PackageId, to: PackageId },
    #[error(transparent)]
    IOError(io::Error),
    #[error(transparent)]
    LuaError(LuaError),
}

impl From<mlua::Error> for PlannerError {
    fn from(err: mlua::Error) -> Self {
        PlannerError::LuaError(err.into())
    }
}

pub type Plan = Acyclic<DiGraph<Package, DependencyType>>;

#[derive(Default)]
pub struct Planner {
    plan: Plan,
    cache: HashMap<u64, NodeIndex>,
}

const MODULE_NAME: &str = "xuehua.planner";

impl Planner {
    #[inline]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn run(lua: &Lua, root: &Path) -> Result<Self, PlannerError> {
        let planner = RefCell::new(Planner::default());
        let get_planner = || {
            planner
                .try_borrow_mut()
                .map_err(|_| mlua::Error::RecursiveMutCallback)
        };

        lua.scope(|scope| {
            let module = lua.create_table()?;
            module.set(
                "package",
                scope.create_function(|_, pkg| {
                    get_planner()?
                        .package(pkg)
                        .map(|node| node.index())
                        .into_lua_err()
                })?,
            )?;
            module.set(
                "repository",
                scope.create_function(|lua, source: u32| {
                    get_planner()?
                        .repository(lua, source.into())
                        .map(|node| node.index())
                        .into_lua_err()
                })?,
            )?;

            lua.register_module(MODULE_NAME, module)?;
            scope.add_destructor(|| {
                if let Err(err) = lua.unload_module(MODULE_NAME) {
                    warn!(error:? = err; "could not unregister {MODULE_NAME}");
                }
            });

            lua.load(fs::read(root)?).exec()
        })?;

        Ok(planner.into_inner())
    }

    pub fn repository(&mut self, _lua: &Lua, _source: NodeIndex) -> Result<NodeIndex, PlannerError> {
        todo!()
    }

    pub fn package(&mut self, pkg: Package) -> Result<NodeIndex, PlannerError> {
        // check cache for node
        let mut hasher = DefaultHasher::new();
        pkg.hash(&mut hasher);
        let hash = hasher.finish();

        Ok(match self.cache.get(&hash) {
            Some(node) => *node,
            None => {
                // insert node if cache miss
                let node = self.plan.add_node(pkg);
                self.cache.insert(hash, node);

                for (d_node, d_type) in self.plan[node].dependencies.clone() {
                    self.plan
                        .try_add_edge(node, NodeIndex::from(d_node), d_type)
                        // TODO: add ids once id resolver done
                        .map_err(|_| PlannerError::Cyclic {
                            from: PackageId::default(),
                            to: PackageId::default(),
                        })?;
                }

                node
            }
        })
    }
}

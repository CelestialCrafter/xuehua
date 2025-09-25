use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
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

use crate::package::{DependencyType, Package, PackageId};

#[derive(Error, Debug)]
pub enum Error {
    #[error("package {package} not found")]
    NotFound { package: PackageId },
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageId },
    #[error("cycle detected from {from} to {to}")]
    Cycle { from: PackageId, to: PackageId },
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub type Plan = Acyclic<DiGraph<Package, DependencyType>>;

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
pub struct Planner {
    plan: Plan,
    cache: HashMap<u64, NodeIndex>,
}

const MODULE_NAME: &str = "xuehua.planner";

impl Planner {
    pub fn new() -> Planner {
        Self {
            plan: Plan::default(),
            cache: HashMap::default(),
        }
    }

    #[inline]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn run(&mut self, lua: &Lua, root: &Path) -> Result<(), Error> {
        let planner = RefCell::new(self);
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

        Ok(())
    }

    pub fn repository(&mut self, _lua: &Lua, _source: NodeIndex) -> Result<NodeIndex, Error> {
        todo!()
    }

    pub fn package(&mut self, pkg: Package) -> Result<NodeIndex, Error> {
        let mut hasher = DefaultHasher::new();
        pkg.hash(&mut hasher);
        let hash = hasher.finish();

        Ok(match self.cache.get(&hash) {
            Some(node) => *node,
            None => {
                let node = self.plan.add_node(pkg);
                self.cache.insert(hash, node);

                for (d_node, d_type) in self.plan[node].dependencies.clone() {
                    self.plan
                        .try_add_edge(node, NodeIndex::from(d_node), d_type)
                        // TODO: add ids once id resolver done
                        .map_err(|_| Error::Cycle {
                            from: PackageId::default(),
                            to: PackageId::default(),
                        })?;
                }

                node
            }
        })
    }
}

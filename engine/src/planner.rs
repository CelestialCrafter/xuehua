use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use log::warn;
use mlua::{ExternalResult, Function, Lua, Table};
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DefaultIx, DiGraph, NodeIndex},
};
use thiserror::Error;

use crate::package::{DependencyType, Package};

#[derive(Error, Debug)]
pub enum Error {
    #[error("node {0:?} not found")]
    NotFound(NodeIndex),
    #[error("package {package} has conflicting definitions")]
    Conflict { package: String },
    #[error("cycle detected from {from} to {to}")]
    Cycle { from: String, to: String },
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
#[derive(Default)]
pub struct Planner {
    plan: Plan,
    registered: HashSet<u64>,
}

const MODULE_NAME: &str = "xuehua.planner";

impl Planner {
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
                "configure",
                scope.create_function(|lua, table: Table| {
                    get_planner()?
                        .configure(
                            lua,
                            table.get::<DefaultIx>("source")?.into(),
                            table.get("destination")?,
                            table.get("modify")?,
                        )
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

    pub fn configure(
        &mut self,
        lua: &Lua,
        source: NodeIndex,
        destination: String,
        modify: Function,
    ) -> Result<NodeIndex, Error> {
        let mut pkg = self
            .plan
            .node_weight(source)
            .ok_or(Error::NotFound(source))?
            .clone();
        pkg.name = destination;
        pkg.configure(lua, modify)?;

        Ok(self.plan.add_node(pkg))
    }

    pub fn package(&mut self, pkg: Package) -> Result<NodeIndex, Error> {
        let mut hasher = DefaultHasher::new();
        pkg.hash(&mut hasher);
        if !self.registered.insert(hasher.finish()) {
            return Err(Error::Conflict { package: pkg.name });
        }

        let node = self.plan.add_node(pkg);
        for (d_node, d_type) in self
            .plan
            .node_weight(node)
            .ok_or(Error::NotFound(node))?
            .dependencies()
            .clone()
        {
            self.plan
                .try_add_edge(node, d_node, d_type)
                // TODO: add ids once id resolver done
                .map_err(|_| Error::Cycle {
                    from: String::default(),
                    to: String::default(),
                })?;
        }

        Ok(node)
    }
}

use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    path::Path,
};

use log::warn;
use mlua::{ExternalResult, Function, Lua, Table, Value};
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DefaultIx, DiGraph, NodeIndex},
};
use thiserror::Error;

use crate::package::{Dependency, LinkTime, LuaNodeIndex, Package, PackageId};

#[derive(Error, Debug)]
pub enum Error {
    #[error("node {0:?} not found")]
    NotFound(NodeIndex),
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageId },
    #[error("cycle detected from package {from:?} to package {to:?}")]
    Cycle { from: PackageId, to: PackageId },
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub type Plan = Acyclic<DiGraph<Package, LinkTime>>;

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
pub struct Planner<'a> {
    plan: Plan,
    registered: HashSet<PackageId>,
    lua: &'a Lua,
}

const MODULE_NAME: &str = "xuehua.planner";

impl<'a> Planner<'a> {
    pub fn new(lua: &'a Lua) -> Self {
        Self {
            plan: Plan::default(),
            registered: HashSet::default(),
            lua,
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
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
        pkg.id.name = destination;
        pkg.configure(lua, modify)?;

        Ok(self.plan.add_node(pkg))
    }

    pub fn package(
        &mut self,
        mut pkg: Package,
        namespace: Vec<String>,
    ) -> Result<NodeIndex, Error> {
        pkg.id.namespace = namespace;

        // ensure no conflicts
        if !self.registered.insert(pkg.id.clone()) {
            return Err(Error::Conflict { package: pkg.id });
        }

        // register node and add dependency edges
        let node = self.plan.add_node(pkg);
        for Dependency {
            node: d_node,
            time: d_time,
        } in self
            .plan
            .node_weight(node)
            .ok_or(Error::NotFound(node))?
            .dependencies()
            .clone()
        {
            let d_node = d_node.into();
            self.plan
                .try_add_edge(node, d_node, d_time)
                .map_err(|_| Error::Cycle {
                    from: self.plan[node].id.clone(),
                    to: self.plan[d_node].id.clone(),
                })?;
        }

        Ok(node)
    }

    pub fn run(&mut self, lua: &Lua, root: &Path) -> Result<(), Error> {
        let namespace = RefCell::new(vec!["root".to_string()]);
        let get_namespace = || {
            namespace
                .try_borrow_mut()
                .map_err(|_| mlua::Error::RecursiveMutCallback)
        };

        let planner = RefCell::new(self);
        let get_planner = || {
            planner
                .try_borrow_mut()
                .map_err(|_| mlua::Error::RecursiveMutCallback)
        };

        lua.scope(|scope| {
            let module = lua.create_table()?;

            module.set(
                "namespace",
                scope.create_function(|_, (name, func): (String, Function)| {
                    // release borrow to allow nested namespace calls
                    get_namespace()?.push(name);
                    let rval = func.call::<Value>(());
                    get_namespace()?.pop();
                    Ok(rval)
                })?,
            )?;

            module.set(
                "package",
                scope.create_function(|_, pkg| {
                    get_planner()?
                        .package(pkg, get_namespace()?.clone())
                        .map(LuaNodeIndex::from)
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
                        .map(LuaNodeIndex::from)
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
}

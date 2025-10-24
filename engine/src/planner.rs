use std::{cell::RefCell, collections::HashSet, fs, path::Path, sync::Arc};

use log::warn;
use mlua::{ExternalResult, Function, Lua, Table, UserData, Value};
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
#[derive(Default)]
pub struct Planner {
    plan: Plan,
    registered: HashSet<PackageId>,
    namespace: RefCell<Vec<Arc<str>>>,
}

const MODULE_NAME: &str = "xuehua.planner";

impl Planner {
    pub fn plan(&self) -> &Plan {
        &self.plan
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

    pub fn package(&mut self, mut pkg: Package) -> Result<NodeIndex, Error> {
        pkg.id.namespace = self.namespace.borrow().clone();

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
}

impl UserData for Planner {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("namespace", |_, this| {
            Ok(this
                .namespace
                .borrow()
                .iter()
                .map(|str| str.to_string())
                .collect::<Vec<_>>())
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("package", |_, this, pkg| {
            this.package(pkg).map(LuaNodeIndex::from).into_lua_err()
        });

        methods.add_method_mut("configure", |lua, this, table: Table| {
            this.configure(
                lua,
                table.get::<DefaultIx>("source")?.into(),
                table.get("destination")?,
                table.get("modify")?,
            )
            .map(LuaNodeIndex::from)
            .into_lua_err()
        });

        methods.add_method("namespace", |_, this, (name, func): (String, Function)| {
            // release borrow to allow nested namespace calls
            this.namespace.borrow_mut().push(name.into());
            let rval = func.call::<Value>(());
            this.namespace.borrow_mut().pop();
            Ok(rval)
        });
    }
}

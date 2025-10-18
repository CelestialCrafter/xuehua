use std::{cell::RefCell, collections::HashSet, io, sync::Arc};

use mlua::{AnyUserData, ExternalResult, FromLua, Function, Lua, Table, UserData, Value};
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DefaultIx, DiGraph, NodeIndex},
};
use thiserror::Error;

use crate::package::{Package, PackageId};

#[derive(Debug, Clone, Copy)]
pub enum LinkTime {
    Runtime,
    Buildtime,
}

impl FromLua for LinkTime {
    fn from_lua(value: mlua::Value, _: &Lua) -> Result<Self, mlua::Error> {
        match value.to_string()?.as_str() {
            "buildtime" => Ok(LinkTime::Buildtime),
            "runtime" => Ok(LinkTime::Runtime),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LinkTime".to_string(),
                message: Some(r#"value is not "buildtime" or "runtime""#.to_string()),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dependency {
    pub node: NodeIndex,
    pub time: LinkTime,
}
impl FromLua for Dependency {
    fn from_lua(value: mlua::Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            node: *table.get::<AnyUserData>("package")?.borrow::<NodeIndex>()?,
            time: table.get("type")?,
        })
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("node {0:?} not found")]
    NotFound(NodeIndex),
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageId },
    #[error("cycle detected from package {from:?} to package {to:?}")]
    Cycle { from: PackageId, to: PackageId },
    #[error(transparent)]
    IOError(#[from] io::Error),
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
    namespace: RefCell<Vec<Arc<str>>>,
    registered: HashSet<PackageId>,
}

const MODULE_NAME: &str = "xuehua.planner";

impl Planner {
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn into_inner(self) -> Plan {
        self.plan
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

    pub fn namespace<T, F: FnOnce() -> T>(&self, name: &str, func: F) -> T {
        // release borrow to allow nested namespace calls
        self.namespace.borrow_mut().push(name.into());
        let rval = func();
        self.namespace.borrow_mut().pop();
        rval
    }

    pub fn package(
        &mut self,
        mut pkg: Package,
        dependencies: Vec<Dependency>,
    ) -> Result<NodeIndex, Error> {
        pkg.id.namespace = self.namespace.borrow().clone();

        // ensure no conflicts
        if !self.registered.insert(pkg.id.clone()) {
            return Err(Error::Conflict { package: pkg.id });
        }

        // register node and add dependency edges
        let node = self.plan.add_node(pkg);
        for dependency in dependencies {
            self.plan
                .try_add_edge(node, dependency.node, dependency.time)
                // maybe use greedy_feedback_arc_set for better errors on cycles
                .map_err(|_| Error::Cycle {
                    from: self.plan[node].id.clone(),
                    to: self.plan[dependency.node].id.clone(),
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
        methods.add_method_mut("package", |lua, this, table: Table| {
            let dependencies = table.get("dependencies")?;
            let pkg = Package::from_lua(Value::Table(table), lua)?;

            this.package(pkg, dependencies)
                .map(AnyUserData::wrap)
                .into_lua_err()
        });

        methods.add_method_mut("configure", |lua, this, table: Table| {
            this.configure(
                lua,
                table.get::<DefaultIx>("source")?.into(),
                table.get("destination")?,
                table.get("modify")?,
            )
            .map(AnyUserData::wrap)
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

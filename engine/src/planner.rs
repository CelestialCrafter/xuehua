use std::{
    collections::HashMap,
    fmt, io,
    str::FromStr,
    sync::{Arc, RwLock},
};

use log::trace;
use mlua::{
    AnyUserData, ExternalResult, FromLua, Function, Lua, Table, UserData, UserDataFields, Value,
};
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

impl fmt::Display for LinkTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinkTime::Runtime => "runtime",
                LinkTime::Buildtime => "buildtime",
            }
        )
    }
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
    NodeNotFound(NodeIndex),
    #[error("package {0} not found")]
    PackageNotFound(PackageId),
    #[error("package {package} has conflicting definitions")]
    Conflict { package: PackageId },
    #[error("cycle detected from package {from:?} to package {to:?}")]
    Cycle { from: PackageId, to: PackageId },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

#[derive(Clone)]
pub struct Namespace(Arc<RwLock<Vec<Arc<str>>>>);

impl Namespace {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(vec!["root".into()])))
    }

    pub fn current(&self) -> Vec<Arc<str>> {
        self.0.read().unwrap().clone()
    }

    pub fn scope<R, S: Into<Arc<str>>>(&self, segment: S, func: impl FnOnce() -> R) -> R {
        let get = || self.0.write().unwrap();
        get().push(segment.into());
        let rval = func();
        get().pop();
        rval
    }
}

impl UserData for Namespace {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("current", |_, this| {
            Ok(this
                .current()
                .into_iter()
                .map(|str| str.to_string())
                .collect::<Vec<_>>())
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("scope", |_, this, (segment, func): (String, Function)| {
            this.scope(segment, || func.call::<Value>(()))
        });
    }

    fn register(registry: &mut mlua::UserDataRegistry<Self>) {
        Self::add_fields(registry);
        Self::add_methods(registry);
    }
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
pub struct Planner {
    plan: Plan,
    registered: HashMap<PackageId, NodeIndex>,
    namespace: Namespace,
}

impl Planner {
    pub fn new() -> Self {
        Self {
            plan: Plan::default(),
            registered: HashMap::default(),
            namespace: Namespace::new(),
        }
    }

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
        let source_pkg = self
            .plan
            .node_weight(source)
            .ok_or(Error::NodeNotFound(source))?;

        let mut dest_pkg = source_pkg.clone();
        dest_pkg.id.name = destination;
        trace!("configuring package {} into {}", source_pkg.id, dest_pkg.id);

        dest_pkg.configure(lua, modify)?;
        Ok(self.plan.add_node(dest_pkg))
    }

    pub fn package(
        &mut self,
        mut pkg: Package,
        dependencies: Vec<Dependency>,
    ) -> Result<NodeIndex, Error> {
        pkg.id.namespace = self.namespace.current();
        trace!("registering package {}", pkg.id);

        // ensure no conflicts
        if self.registered.contains_key(&pkg.id) {
            return Err(Error::Conflict { package: pkg.id });
        }

        // register node and add dependency edges
        let id = pkg.id.clone();
        let node = self.plan.add_node(pkg);
        self.registered.insert(id, node);

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

    pub fn resolve(&self, id: &PackageId) -> Option<NodeIndex> {
        self.registered.get(id).copied()
    }
}

impl UserData for Planner {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("namespace", |_, this| Ok(this.namespace.clone()));
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
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

        methods.add_method_mut("package", |lua, this, table: Table| {
            let dependencies = table.get("dependencies")?;
            let pkg = Package::from_lua(Value::Table(table), lua)?;

            this.package(pkg, dependencies)
                .map(AnyUserData::wrap)
                .into_lua_err()
        });

        methods.add_method("resolve", |_, this, id: String| {
            let id = PackageId::from_str(&id).into_lua_err()?;
            Ok(this.resolve(&id).map(AnyUserData::wrap))
        });
    }
}

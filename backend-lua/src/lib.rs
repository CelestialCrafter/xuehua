mod logger;

use std::{fmt::Debug, io, path::Path, str::FromStr};

use log::warn;
use mlua::{
    AnyUserData, ExternalResult, Function, Lua, LuaSerdeExt, Table, UserData, UserDataRegistry,
    Value,
};
use petgraph::graph::{DefaultIx, NodeIndex};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use xh_engine::{
    backend::Backend,
    package::{Dependency, DispatchRequest, LinkTime, Metadata, Package, PackageName},
    planner::{Config, NamespaceTracker, Planner, Unfrozen},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("node {0:?} was not registered in the builder")]
    UnregisteredNode(NodeIndex),
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

fn conv_dependency(table: Table) -> Result<Dependency, Error> {
    Ok(Dependency {
        node: table.get::<AnyUserData>("node")?.take()?,
        time: LinkTime::from_str(&table.get::<String>("time")?).into_lua_err()?,
    })
}

fn conv_request(table: Table) -> Result<DispatchRequest<LuaBackend>, Error> {
    Ok(DispatchRequest {
        executor: table.get::<String>("executor")?.into(),
        payload: table.get("payload")?,
    })
}

fn conv_package(table: Table) -> Result<Package<LuaBackend>, Error> {
    Ok(Package {
        name: Default::default(),
        metadata: Metadata,
        requests: table
            .get::<Option<Vec<Table>>>("requests")?
            .unwrap_or_default()
            .into_iter()
            .map(conv_request)
            .collect::<Result<_, _>>()?,
        dependencies: table
            .get::<Option<Vec<Table>>>("dependencies")?
            .unwrap_or_default()
            .into_iter()
            .map(conv_dependency)
            .collect::<Result<_, _>>()?,
    })
}

fn conv_config(table: Table) -> Result<Config<LuaBackend>, Error> {
    let identifier = table.get::<String>("name")?;
    let defaults = table.get::<Option<Value>>("defaults")?.unwrap_or_default();
    let apply = table.get::<Function>("apply")?;
    let apply = move |value: Value| apply.call(value).map_err(Into::into).and_then(conv_package);

    Ok(Config::new(identifier, defaults, apply))
}

struct LuaNamespaceTracker(NamespaceTracker);

impl UserData for LuaNamespaceTracker {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("current", |_, this| {
            Ok(this
                .0
                .current()
                .into_iter()
                .map(|str| str.to_string())
                .collect::<Vec<_>>())
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("scope", |_, this, (segment, func): (String, Function)| {
            this.0.scope(segment, || func.call::<Value>(()))
        });
    }

    fn register(registry: &mut mlua::UserDataRegistry<Self>) {
        Self::add_fields(registry);
        Self::add_methods(registry);
    }
}

fn planner_userdata(registry: &mut UserDataRegistry<Planner<Unfrozen<LuaBackend>>>) {
    fn add_fields<F: mlua::UserDataFields<Planner<Unfrozen<LuaBackend>>>>(fields: &mut F) {
        fields.add_field_method_get("namespace", |_, this| {
            Ok(LuaNamespaceTracker(this.namespace()))
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Planner<Unfrozen<LuaBackend>>>>(methods: &mut M) {
        methods.add_method_mut("configure", |_, this, table: Table| {
            this.configure(
                table.get::<DefaultIx>("source")?.into(),
                table.get::<String>("identifier")?.into(),
                {
                    let func: Function = table.get("modify")?;
                    move |value| func.call(value).map_err(Into::into)
                },
            )
            .expect("source should be a registered node")
            .map(AnyUserData::wrap)
            .into_lua_err()
        });

        methods.add_method_mut("package", |_, this, table: Table| {
            this.register(conv_config(table).into_lua_err()?)
                .map(AnyUserData::wrap)
                .into_lua_err()
        });

        methods.add_method("resolve", |_, this, id: String| {
            let id = PackageName::from_str(&id).into_lua_err()?;
            Ok(this.resolve(&id).map(AnyUserData::wrap))
        });
    }

    add_fields(registry);
    add_methods(registry);
}

#[derive(Debug)]
pub struct LuaBackend {
    lua: Lua,
}

impl LuaBackend {
    #[inline]
    pub fn new() -> Result<Self, Error> {
        let lua = Lua::new();
        logger::register_module(&lua)?;
        lua.register_userdata_type(planner_userdata)?;

        Ok(Self { lua })
    }

    pub fn plan(&self, planner: &mut Planner<Unfrozen<Self>>, project: &Path) -> Result<(), Error> {
        let chunk = self
            .lua
            .load(std::fs::read(project.join("main.lua"))?)
            .into_function()?;

        self.lua
            .scope(|scope| {
                with_module(
                    &self.lua,
                    &scope,
                    "xuehua.planner",
                    scope.create_any_userdata_ref_mut(planner)?,
                )?;

                chunk.call::<()>(())
            })
            .map_err(Into::into)
    }
}

impl Backend for LuaBackend {
    type Error = Error;
    type Value = mlua::Value;

    fn serialize<T: Serialize>(&self, value: &T) -> Result<Self::Value, Self::Error> {
        self.lua.to_value(value).map_err(Into::into)
    }

    fn deserialize<T: DeserializeOwned>(&self, value: Self::Value) -> Result<T, Self::Error> {
        self.lua.from_value(value).map_err(Into::into)
    }
}

fn with_module<'scope, 'env>(
    lua: &'env mlua::Lua,
    scope: &'scope mlua::Scope<'scope, 'env>,
    name: &'env str,
    value: impl mlua::IntoLua,
) -> Result<(), mlua::Error> {
    lua.register_module(name, value)?;
    scope.add_destructor(move || {
        if let Err(err) = lua.unload_module(name) {
            warn!("could not unregister {name}: {err}");
        }
    });

    Ok(())
}

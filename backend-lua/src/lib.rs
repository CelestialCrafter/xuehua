mod logger;

use std::{
    fmt::{self, Debug},
    path::Path,
    str::FromStr,
};

use log::warn;
use mlua::{
    AnyUserData, ExternalResult, Function, Lua, LuaSerdeExt, Table, UserData, UserDataRegistry,
    Value,
};
use petgraph::graph::{DefaultIx, NodeIndex};
use serde::{Serialize, de::DeserializeOwned};
use xh_engine::{
    backend::Backend,
    package::{Dependency, DispatchRequest, LinkTime, Metadata, Package, PackageName},
    planner::{Config, NamespaceTracker, Planner, Unfrozen},
};
use xh_reports::{compat::StdCompat, impl_compat, prelude::*};

#[derive(Debug, IntoReport)]
#[message("node was not registered in the builder")]
#[context(debug: 0 = node)]
pub struct UnregisteredNodeError(NodeIndex);

#[derive(Default, Debug, IntoReport)]
#[message("could not run lua backend")]
pub struct Error;

impl_compat!(
    LuaCompat,
    (mlua::Error, |error| {
        let mut frames = vec![];

        fn conversion_error(
            from: impl fmt::Display,
            to: impl fmt::Display,
            frames: &mut Vec<Frame>,
        ) {
            frames.extend([
                Frame::context("from", from),
                Frame::context("to", to),
                Frame::suggestion("provide the correct types during conversion"),
            ]);
        }

        match &error {
            mlua::Error::BadArgument {
                to,
                pos,
                name,
                cause: _,
            } => {
                if let Some(func) = to {
                    frames.push(Frame::context("function", func))
                }

                if let Some(arg) = name {
                    frames.push(Frame::context("argument", arg));
                }

                frames.extend([
                    Frame::context("position", pos),
                    Frame::suggestion("provide the correct function arguments"),
                ]);
            }
            mlua::Error::ToLuaConversionError {
                from,
                to,
                message: _,
            } => conversion_error(from, to, &mut frames),
            mlua::Error::FromLuaConversionError {
                from,
                to,
                message: _,
            } => conversion_error(from, to, &mut frames),
            _ => (),
        }

        Report::from_error(error).with_frames(frames)
    })
);

fn conv_dependency(table: Table) -> StdResult<Dependency, mlua::Error> {
    Ok(Dependency {
        node: table.get::<AnyUserData>("node")?.take()?,
        time: LinkTime::from_str(&table.get::<String>("time")?)
            .into_error()
            .into_lua_err()?,
    })
}

fn conv_request(table: Table) -> StdResult<DispatchRequest<LuaBackend>, mlua::Error> {
    Ok(DispatchRequest {
        executor: table.get::<String>("executor")?.into(),
        payload: table.get("payload")?,
    })
}

fn conv_package(table: Table) -> StdResult<Package<LuaBackend>, mlua::Error> {
    Ok(Package {
        name: Default::default(),
        metadata: Metadata,
        requests: table
            .get::<Option<Vec<Table>>>("requests")?
            .unwrap_or_default()
            .into_iter()
            .map(conv_request)
            .collect::<StdResult<_, _>>()?,
        dependencies: table
            .get::<Option<Vec<Table>>>("dependencies")?
            .unwrap_or_default()
            .into_iter()
            .map(conv_dependency)
            .collect::<StdResult<_, _>>()?,
    })
}

fn conv_config(table: Table) -> StdResult<Config<LuaBackend>, mlua::Error> {
    let identifier = table.get::<String>("name")?;
    let defaults = table.get::<Option<Value>>("defaults")?.unwrap_or_default();

    let apply = table.get::<Function>("apply")?;
    let apply = move |value: Value| apply.call(value).and_then(conv_package).compat().wrap();

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
                    move |value| func.call(value).wrap()
                },
            )
            .expect("source should be a registered node")
            .map(AnyUserData::wrap)
            .into_error()
            .into_lua_err()
        });

        methods.add_method_mut("package", |_, this, table: Table| {
            this.register(conv_config(table).into_lua_err()?)
                .map(AnyUserData::wrap)
                .into_error()
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
        logger::register_module(&lua).compat().wrap()?;
        lua.register_userdata_type(planner_userdata)
            .compat()
            .wrap()?;

        Ok(Self { lua })
    }

    pub fn plan(&self, planner: &mut Planner<Unfrozen<Self>>, project: &Path) -> Result<(), Error> {
        let chunk = self
            .lua
            .load(std::fs::read(project.join("main.lua")).compat().wrap()?)
            .into_function()
            .compat()
            .wrap()?;

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
            .compat()
            .wrap()
    }
}

impl Backend for LuaBackend {
    type Error = Error;
    type Value = mlua::Value;

    fn serialize<T: Serialize>(&self, value: &T) -> Result<Self::Value, Self::Error> {
        self.lua.to_value(value).compat().wrap()
    }

    fn deserialize<T: DeserializeOwned>(&self, value: Self::Value) -> Result<T, Self::Error> {
        self.lua.from_value(value).compat().wrap()
    }
}

fn with_module<'scope, 'env>(
    lua: &'env mlua::Lua,
    scope: &'scope mlua::Scope<'scope, 'env>,
    name: &'env str,
    value: impl mlua::IntoLua,
) -> StdResult<(), mlua::Error> {
    lua.register_module(name, value)?;
    scope.add_destructor(move || {
        if let Err(err) = lua.unload_module(name) {
            warn!("could not unregister {name}: {err}");
        }
    });

    Ok(())
}

mod logger;

use std::{path::Path, str::FromStr};

use log::warn;
use mlua::{
    AnyUserData, ExternalResult, Function, Lua, Table, UserData, UserDataRegistry,
    Value as LuaValue,
};
use petgraph::graph::{DefaultIx, NodeIndex};
use xh_engine::{
    backend::Backend,
    encoding::to_value,
    package::{Dependency, DispatchRequest, LinkTime, Metadata, Package, PackageName},
    planner::{
        NamespaceTracker, Planner, Unfrozen,
        config::{Config, ConfigManager},
    },
};
use xh_reports::prelude::*;

#[derive(Default, Debug, IntoReport)]
#[message("could not run lua backend")]
pub struct Error;

fn conv_dependency(table: Table) -> StdResult<Dependency, mlua::Error> {
    Ok(Dependency {
        name: table.get::<AnyUserData>("package")?.take()?,
        time: LinkTime::from_str(&table.get::<String>("time")?)
            .into_error()
            .into_lua_err()?,
    })
}

fn conv_request(table: Table) -> Result<DispatchRequest, Error> {
    Ok(DispatchRequest {
        executor: table.get::<String>("executor").wrap()?.into(),
        payload: to_value(table.get::<LuaValue>("payload").wrap()?).wrap()?,
    })
}

fn conv_package(table: Table) -> Result<Package, Error> {
    Ok(Package {
        name: Default::default(),
        metadata: Metadata,
        requests: table
            .get::<Option<Vec<Table>>>("requests")
            .wrap()?
            .unwrap_or_default()
            .into_iter()
            .map(conv_request)
            .collect::<Result<_, _>>()?,
        dependencies: table
            .get::<Option<Vec<Table>>>("dependencies")
            .wrap()?
            .unwrap_or_default()
            .into_iter()
            .map(conv_dependency)
            .collect::<StdResult<_, _>>()
            .wrap()?,
    })
}

fn conv_config(table: Table) -> StdResult<Config<LuaBackend>, mlua::Error> {
    let defaults = table
        .get::<Option<LuaValue>>("defaults")?
        .unwrap_or_default();

    let apply = table.get::<Function>("apply")?;
    let apply = move |value: LuaValue| apply.call(value).wrap().and_then(conv_package);

    Ok(Config::new(defaults, apply))
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
            this.0.scope(segment, || func.call::<LuaValue>(()))
        });
    }

    fn register(registry: &mut mlua::UserDataRegistry<Self>) {
        Self::add_fields(registry);
        Self::add_methods(registry);
    }
}

struct LuaConfigManager<'a> {
    inner: ConfigManager<'a, LuaBackend>,
    namespace: NamespaceTracker,
}

impl UserData for LuaConfigManager<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("namespace", |_, this| {
            Ok(LuaNamespaceTracker(this.namespace.clone()))
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("configure", |_, this, table: Table| {
            let source = NodeIndex::from(table.get::<DefaultIx>("source")?);
            let dest = PackageName {
                identifier: table.get::<String>("identifier")?.into(),
                namespace: this.namespace.current(),
            };

            let modify = {
                let func: Function = table.get("modify")?;
                move |value| func.call(value).wrap()
            };

            this.inner
                .configure(&source, dest, modify)
                .expect("source should be a registered node")
                .map(AnyUserData::wrap)
                .into_error()
                .into_lua_err()
        });

        methods.add_method_mut("package", |_, this, table: Table| {
            let name = PackageName {
                identifier: table.get::<String>("identifier")?.into(),
                namespace: this.namespace.current(),
            };
            let config = conv_config(table).into_lua_err()?;

            this.inner
                .register(name, config)
                .into_error()
                .into_lua_err()
        });
    }

    fn register(registry: &mut UserDataRegistry<Self>) {
        Self::add_fields(registry);
        Self::add_methods(registry);
    }
}

#[derive(Debug)]
pub struct LuaBackend {
    lua: Lua,
}

impl LuaBackend {
    #[inline]
    pub fn new() -> Result<Self, Error> {
        let lua = Lua::new();
        logger::register_module(&lua).wrap()?;

        Ok(Self { lua })
    }
}

impl Backend for LuaBackend {
    type Error = Error;
    type Value = LuaValue;

    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Error> {
        let chunk = self
            .lua
            .load(std::fs::read(project.join("main.lua")).wrap()?)
            .into_function()
            .wrap()?;

        let manager = LuaConfigManager {
            inner: ConfigManager::new(planner),
            namespace: NamespaceTracker::default(),
        };

        self.lua
            .scope(|scope| {
                with_module(
                    &self.lua,
                    &scope,
                    "xuehua.planner",
                    scope.create_userdata(manager)?,
                )?;

                chunk.call::<()>(())
            })
            .wrap()?;

        Ok(())
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

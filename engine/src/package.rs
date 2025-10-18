pub mod id;
pub mod manifest;

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table};

pub use crate::package::id::PackageId;

#[derive(Debug, Clone)]
pub struct Metadata;

#[derive(Debug, Clone)]
struct Partial {
    metadata: Metadata,
    build: Function,
}

impl FromLua for Partial {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            metadata: Metadata,
            build: table.get("build")?,
        })
    }
}

#[derive(Debug, Clone)]
struct Config {
    current: serde_json::Value,
    apply: Function,
}

impl Config {
    fn configure(&mut self, lua: &Lua, modify: Function) -> Result<Partial, mlua::Error> {
        let new = modify.call::<mlua::Value>(lua.to_value(&self.current)?)?;
        let partial = self.apply.call(&new)?;
        self.current = lua.from_value(new)?;

        Ok(partial)
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub id: PackageId,
    partial: Partial,
    config: Config,
}

impl Package {
    pub fn configure(&mut self, lua: &Lua, modify: Function) -> Result<(), mlua::Error> {
        self.partial = self.config.configure(lua, modify)?;
        Ok(())
    }

    pub fn build(&self) -> impl Future<Output = Result<(), mlua::Error>> {
        self.partial.build.call_async(())
    }

    pub fn metadata(&self) -> &Metadata {
        &self.partial.metadata
    }
}

impl FromLua for Package {
    fn from_lua(value: mlua::Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        let name = table.get("name")?;

        let mut config = Config {
            current: serde_json::Value::Null,
            apply: table.get("configure")?,
        };

        let partial = config.configure(
            lua,
            lua.create_function::<_, _, mlua::Value>(move |_, _: mlua::Value| {
                table.get("defaults")
            })?,
        )?;

        Ok(Self {
            id: PackageId {
                name,
                namespace: Default::default(),
            },
            partial,
            config,
        })
    }
}

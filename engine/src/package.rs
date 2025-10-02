use std::hash::{self, Hash};

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table};
use serde::Deserialize;

pub type PackageId = String;

#[derive(Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub struct Metadata {}

#[derive(Deserialize, Debug, Clone, Copy, Hash, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DependencyType {
    Buildtime,
    Runtime,
}

#[derive(Debug, Clone)]
struct Partial {
    dependencies: Vec<(u32, DependencyType)>,
    metadata: Metadata,
    build: Function,
}

impl FromLua for Partial {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            dependencies: lua.from_value(table.get("dependencies")?)?,
            metadata: lua.from_value(table.get("metadata")?)?,
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

impl hash::Hash for Package {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.partial.dependencies.hash(state);
        self.partial.metadata.hash(state);
        self.config.current.hash(state);
    }
}

impl Package {
    pub fn configure(&mut self, lua: &Lua, modify: Function) -> Result<(), mlua::Error> {
        self.partial = self.config.configure(lua, modify)?;
        Ok(())
    }

    pub fn build(&self) -> Result<(), mlua::Error> {
        self.partial.build.call(())
    }

    pub fn dependencies(&self) -> &Vec<(u32, DependencyType)> {
        &self.partial.dependencies
    }

    pub fn metadata(&self) -> &Metadata {
        &self.partial.metadata
    }
}

impl FromLua for Package {
    fn from_lua(value: mlua::Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        let id = table.get("id")?;
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
            id,
            partial,
            config,
        })
    }
}

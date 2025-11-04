pub mod id;
pub mod manifest;

use std::fmt;

use mlua::{AnyUserData, FromLua, Function, Lua, LuaSerdeExt, Table};
use petgraph::graph::NodeIndex;

pub use crate::package::id::PackageId;

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

#[derive(Debug, Clone)]
pub struct Metadata;

#[derive(Debug, Clone)]
struct Partial {
    metadata: Metadata,
    build: Function,
    dependencies: Vec<Dependency>,
}

impl FromLua for Partial {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;

        let dependencies = table.get::<Option<_>>("dependencies")?.unwrap_or_default();
        let build = match table.get::<Option<_>>("build")? {
            Some(func) => func,
            None => lua.create_function(|_, ()| Ok(()))?,
        };

        Ok(Self {
            metadata: Metadata,
            build,
            dependencies,
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

    pub fn build(&self) -> &Function {
        &self.partial.build
    }

    pub fn metadata(&self) -> &Metadata {
        &self.partial.metadata
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
       &self.partial.dependencies
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

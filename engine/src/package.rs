pub mod id;
pub mod manifest;

use mlua::{AnyUserData, FromLua, Function, Lua, LuaSerdeExt, Table, UserData};
use petgraph::graph::NodeIndex;

use crate::package::id::Id;

#[derive(Debug, Clone, Copy)]
pub struct LuaNodeIndex(NodeIndex);

impl From<NodeIndex> for LuaNodeIndex {
    fn from(value: NodeIndex) -> Self {
        Self(value)
    }
}

impl From<LuaNodeIndex> for NodeIndex {
    fn from(value: LuaNodeIndex) -> Self {
        value.0
    }
}

impl UserData for LuaNodeIndex {}

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
    pub node: LuaNodeIndex,
    pub time: LinkTime,
}

impl FromLua for Dependency {
    fn from_lua(value: mlua::Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            node: *table
                .get::<AnyUserData>("package")?
                .borrow::<LuaNodeIndex>()?,
            time: table.get("type")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Metadata;

#[derive(Debug, Clone)]
struct Partial {
    dependencies: Vec<Dependency>,
    metadata: Metadata,
    build: Function,
}

impl FromLua for Partial {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            dependencies: table.get("dependencies")?,
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
    pub id: Id,
    partial: Partial,
    config: Config,
}

impl Package {
    pub fn configure(&mut self, lua: &Lua, modify: Function) -> Result<(), mlua::Error> {
        self.partial = self.config.configure(lua, modify)?;
        Ok(())
    }

    pub fn build(&self) -> Result<(), mlua::Error> {
        self.partial.build.call(())
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.partial.dependencies
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
            id: Id {
                name,
                namespace: Default::default(),
            },
            partial,
            config,
        })
    }
}

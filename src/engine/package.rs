use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table, Value};
use serde::Deserialize;

pub type PackageId = String;

#[derive(Deserialize, Debug)]
pub struct PackageMetadata {}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DependencyType {
    Buildtime,
    Runtime,
}

#[derive(Deserialize, Debug)]
pub struct PackageDependency {
    pub id: PackageId,
    #[serde(rename = "type")]
    pub dependency_type: DependencyType,
}

#[derive(Debug)]
pub struct Package {
    pub id: PackageId,
    pub dependencies: Vec<PackageDependency>,
    pub metadata: PackageMetadata,
    // TODO: make this field private, and then create a wrapper function
    pub build: Function,
}

impl FromLua for Package {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            id: table.get("id")?,
            dependencies: lua.from_value(table.get("dependencies")?)?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
        })
    }
}

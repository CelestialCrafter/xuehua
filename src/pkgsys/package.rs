pub mod build;
pub mod dependencies;

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table, Value};
use serde::Deserialize;

use crate::pkgsys::package::dependencies::PackageDependency;

#[derive(Deserialize, Debug)]
pub struct PackageMetadata;

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<PackageDependency>,
    pub metadata: PackageMetadata,
    pub build: Function,
}

impl FromLua for Package {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            name: table.get("name")?,
            dependencies: table.get("dependencies")?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
        })
    }
}

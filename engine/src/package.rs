use std::hash::{self, Hash};

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table, Value};
use serde::Deserialize;
use thiserror::Error;

pub type PackageId = String;

#[derive(Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub struct PackageMetadata {}

#[derive(Deserialize, Debug, Clone, Copy, Hash, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DependencyType {
    Buildtime,
    Runtime,
}

#[derive(Error, Debug)]
pub enum PackageConfigurationError {
    #[error("package {0} does not support configuration")]
    Unsupported(PackageId),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

#[derive(Debug, Clone)]
pub struct Package {
    pub id: PackageId,
    pub dependencies: Vec<(u32, DependencyType)>,
    pub metadata: PackageMetadata,
    pub build: Function,
    pub configure: Option<Function>,
}

impl hash::Hash for Package {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.dependencies.hash(state);
        self.metadata.hash(state);

        self.build.dump(true).hash(state);
        if let Some(func) = self.configure.as_ref() {
            func.dump(true).hash(state);
        }
    }
}

impl Package {
    pub fn configure(mut self, inputs: Value) -> Result<Self, PackageConfigurationError> {
        let new: Package = self
            .configure
            .as_ref()
            .ok_or(PackageConfigurationError::Unsupported(self.id.clone()))?
            .call(inputs)?;

        self.dependencies = new.dependencies;
        self.metadata = new.metadata;
        self.build = new.build;

        Ok(self)
    }
}

impl FromLua for Package {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            id: table.get("id")?,
            dependencies: lua.from_value(table.get("dependencies")?)?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
            configure: table.get("configure")?,
        })
    }
}

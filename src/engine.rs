pub mod planner;
pub mod executor;

use std::{convert::Infallible, fmt, str::FromStr};

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table, Value};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Id {
    pub package: String,
    pub namespaces: Vec<String>,
}

impl Id {
    pub fn namespaces_string(&self) -> String {
        self.namespaces.join(".")
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.package, self.namespaces_string())
    }
}

impl FromStr for Id {
    type Err = Infallible;

    // <name>[@<namespace>[.<namespace>]*]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(2, '@');
        Ok(Self {
            package: parts
                .next()
                .expect("splitn should return at least 1 item")
                .to_string(),
            namespaces: parts
                .next()
                .unwrap_or("")
                .split('.')
                .map(|v| v.to_string())
                .collect(),
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct PackageMetadata {}

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<Id>,
    pub metadata: PackageMetadata,
    // TODO: make this field private, and then create a wrapper function
    pub build: Function,
    _lua: Lua,
}

impl FromLua for Package {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            name: table.get("name")?,
            dependencies: lua.from_value(table.get("dependencies")?)?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
            // keep a strong reference to lua so `build` can run
            _lua: lua.clone(),
        })
    }
}

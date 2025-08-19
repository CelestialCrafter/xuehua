use std::fs;

use eyre::{OptionExt, Report, Result};
use mlua::{Lua, LuaSerdeExt, StdLib, Value};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(try_from = "String")]
struct PackageId {
    name: String,
    namespace: String,
    version: String,
}

impl TryFrom<String> for PackageId {
    type Error = Report;

    // namespace/name@version
    fn try_from(value: String) -> Result<Self> {
        let (namespace, rest) = value.split_once("/").ok_or_eyre("no / delimiter")?;
        let (name, version) = rest.split_once("@").ok_or_eyre("no @ delimiter")?;

        Ok(Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })
    }
}

#[derive(Deserialize, Debug)]
struct Package {
    id: PackageId,
    build: String,
}

fn main() {
    let lua = Lua::new();
    lua.load_std_libs(StdLib::ALL_SAFE)
        .expect("could not load stdlibs");
    let value: Value = lua
        .load(fs::read("./package.lua").expect("could not open package.lua"))
        .eval()
        .expect("could not eval package.lua");
    let package: Package = lua
        .from_value(value)
        .expect("could not convert lua value to package");

    println!("{:?}", package);
}

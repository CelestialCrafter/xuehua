pub mod options;

use std::{fs, str::FromStr};

use eyre::{OptionExt, Report, Result};
use mlua::{Lua, LuaSerdeExt, StdLib, Value};
use serde::Deserialize;

use crate::options::options;

#[derive(Deserialize, Debug, Clone)]
pub struct PackageId {
    name: String,
    namespace: String,
    version: String,
}

impl FromStr for PackageId {
    type Err = Report;

    // namespace/name@version
    fn from_str(s: &str) -> Result<Self> {
        let (namespace, rest) = s.split_once("/").ok_or_eyre("no / delimiter")?;
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
    println!("{:?}", options().run());

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

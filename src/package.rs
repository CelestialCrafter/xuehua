use eyre::{Context, OptionExt, Report, eyre};
use mlua::{AsChunk, Lua, LuaSerdeExt, StdLib};
use serde::Deserialize;
use std::{ops::Deref, str::FromStr};

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(try_from = "String")]
pub struct Id {
    pub name: String,
    pub namespace: String,
    pub version: String,
}

impl FromStr for Id {
    type Err = Report;

    // <namespace>/<name>[@version]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (namespace, rest) = s.split_once("/").ok_or_eyre("no / delimiter")?;
        let (name, version) = rest.split_once("@").unwrap_or((rest, "latest"));

        Ok(Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })
    }
}

impl TryFrom<String> for Id {
    type Error = <Self as FromStr>::Err;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

#[derive(Deserialize, Debug)]
pub struct Package {
    pub id: Id,
    pub build_command: Vec<String>,
}

#[derive(Debug)]
pub enum BuildError {
    LuaError(Report),
    InstructionError(Report),
}

impl Deref for BuildError {
    type Target = Report;

    fn deref(&self) -> &Self::Target {
        match self {
            BuildError::LuaError(err) => err,
            BuildError::InstructionError(err) => err,
        }
    }
}

impl From<mlua::Error> for BuildError {
    fn from(value: mlua::Error) -> Self {
        BuildError::LuaError(eyre!(value.to_string()))
    }
}

fn eval_pkg(source: impl AsChunk) -> Result<Package, BuildError> {
    let transform_err = |err| BuildError::from(err);
    let lua = Lua::new();

    // TODO: make sure there's no direct access to the filesystem
    lua.load_std_libs(StdLib::ALL_SAFE).map_err(transform_err)?;

    let evalled = lua.load(source).eval().map_err(transform_err)?;
    let pkg = lua.from_value(evalled).map_err(transform_err)?;

    Ok(pkg)
}

pub fn build(source: impl AsChunk) -> Result<(), BuildError> {
    let pkg = eval_pkg(source)?;
    let cmd = pkg.build_command;

    let (program, args) = cmd.split_first().ok_or(BuildError::InstructionError(eyre!(
        "invalid instruction: {cmd:?} (is there an executable?)"
    )))?;

    let output = duct::cmd(program, args)
        .stderr_capture()
        .unchecked()
        .run()
        .wrap_err(format!("could not run command: {cmd:?}"))
        .map_err(BuildError::InstructionError)?;

    if !output.status.success() {
        return Err(BuildError::InstructionError(
            eyre!(
                "{cmd:?} returned non-zero exit code: {}",
                output.status.code().unwrap_or(-1)
            )
            // TODO: truncate stderr, gate full stderr behind log level
            .wrap_err(String::from_utf8_lossy(&output.stderr).into_owned()),
        ));
    }

    Ok(())
}

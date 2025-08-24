use std::ops::Deref;
use std::{convert::Infallible, fmt::Display, str::FromStr};

use eyre::{Report, eyre};
use serde::Deserialize;

pub mod namespace;
pub mod package;

#[derive(Debug, Deserialize, Clone)]
pub struct Id {
    pub package: String,
    pub namespaces: Vec<String>,
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.package, self.namespaces.join("."))
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

#[derive(Debug)]
pub enum PkgSysError {
    LuaError(Report),
    InstructionError(Report),
    NotFound(Report),
    Conflict(Report),
    Cyclic(Report),
    Other(Report),
}

impl Deref for PkgSysError {
    type Target = Report;

    fn deref(&self) -> &Self::Target {
        match self {
            PkgSysError::LuaError(err)
            | PkgSysError::InstructionError(err)
            | PkgSysError::NotFound(err)
            | PkgSysError::Conflict(err)
            | PkgSysError::Other(err)
            | PkgSysError::Cyclic(err) => err,
        }
    }
}

impl From<mlua::Error> for PkgSysError {
    fn from(value: mlua::Error) -> Self {
        PkgSysError::LuaError(eyre!(value.to_string()))
    }
}

impl Into<mlua::Error> for PkgSysError {
    fn into(self) -> mlua::Error {
        mlua::Error::runtime(self.to_string())
    }
}

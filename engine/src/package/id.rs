use std::{fmt, str::FromStr, sync::Arc};

use thiserror::Error;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PackageId {
    pub name: String,
    pub namespace: Vec<Arc<str>>,
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.namespace.join("/"))
    }
}

#[derive(Error, Debug)]
#[error("could not parse id")]
pub struct ParseIdError;

impl FromStr for PackageId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (name, namespace) = s.split_once("@").ok_or(ParseIdError)?;

        Ok(Self {
            name: name.to_string(),
            namespace: namespace.split("/").map(|s| s.into()).collect(),
        })
    }
}

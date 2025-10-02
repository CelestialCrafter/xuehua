use std::{fmt, str::FromStr};

use thiserror::Error;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Id {
    pub name: String,
    pub namespace: Vec<String>,
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.namespace.join("/"))
    }
}

#[derive(Error, Debug)]
#[error("could not parse id")]
pub struct ParseIdError;

impl FromStr for Id {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (name, namespace) = s.split_once("@").ok_or(ParseIdError)?;

        Ok(Self {
            name: name.to_string(),
            namespace: namespace.split("/").map(|s| s.to_string()).collect(),
        })
    }
}

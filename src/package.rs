pub mod build;
pub mod namespace;

use serde::Deserialize;
use std::{convert::Infallible, fmt::Display, str::FromStr};

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

#[derive(Default, Deserialize, Debug)]
pub struct Dependencies {
    pub build: Vec<Id>,
    pub runtime: Vec<Id>,
}

#[derive(Deserialize, Debug)]
pub struct Package {
    #[serde(default)]
    pub dependencies: Dependencies,
    pub instructions: Vec<String>,
}

pub mod build;

use eyre::{OptionExt, Report};
use serde::Deserialize;
use std::{path::PathBuf, str::FromStr};

#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "String")]
pub struct Id {
    pub name: String,
    pub namespace: String,
}

impl FromStr for Id {
    type Err = Report;

    // <name>@<namespace>
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (namespace, name) = s.split_once("@").ok_or_eyre("no @ delimiter")?;

        Ok(Self {
            name: name.to_string(),
            namespace: namespace.to_string(),
        })
    }
}

// satisfy serde
impl TryFrom<String> for Id {
    type Error = <Self as FromStr>::Err;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

#[derive(Default, Deserialize, Debug)]
pub struct Dependencies {
    pub build: Vec<PathBuf>,
    pub runtime: Vec<PathBuf>,
}

#[derive(Deserialize, Debug)]
pub struct Package {
    pub id: Id,
    #[serde(default)]
    pub dependencies: Dependencies,
    pub instructions: Vec<String>,
}

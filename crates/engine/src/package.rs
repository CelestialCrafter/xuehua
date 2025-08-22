pub mod manifest;

use std::{fmt, result::Result as StdResult, str::FromStr};

use petgraph::graph::NodeIndex;
use smol_str::SmolStr;
use xh_reports::prelude::*;

use crate::encoding::Value;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum LinkTime {
    Runtime,
    Buildtime,
}

impl fmt::Display for LinkTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinkTime::Runtime => "runtime",
                LinkTime::Buildtime => "buildtime",
            }
        )
    }
}

#[derive(Debug, IntoReport)]
#[message("could not parse link time")]
#[suggestion("provide \"buildtime\" or \"runtime\"")]
#[context(found)]
pub struct LinkTimeParseError {
    found: SmolStr,
}

impl FromStr for LinkTime {
    type Err = Report<LinkTimeParseError>;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        match s {
            "buildtime" => Ok(LinkTime::Buildtime),
            "runtime" => Ok(LinkTime::Runtime),
            _ => Err(LinkTimeParseError { found: s.into() }.into_report()),
        }
    }
}

#[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
pub struct PackageName {
    pub identifier: SmolStr,
    pub namespace: Vec<SmolStr>,
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_empty() {
            self.identifier.fmt(f)
        } else {
            write!(f, "{}@{}", self.identifier, self.namespace.join("/"))
        }
    }
}

impl FromStr for PackageName {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        let (identifier, namespace) = s.split_once("@").unwrap_or((s, Default::default()));

        let identifier = SmolStr::new(identifier);
        let namespace = namespace.split('/').map(Into::into).collect::<Vec<_>>();

        Ok(Self {
            identifier,
            namespace,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Metadata;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct DispatchRequest {
    pub executor: SmolStr,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Dependency {
    pub node: NodeIndex,
    pub time: LinkTime,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Package {
    pub name: PackageName,
    pub metadata: Metadata,
    pub requests: Vec<DispatchRequest>,
    pub dependencies: Vec<Dependency>,
}

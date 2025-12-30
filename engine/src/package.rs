pub mod manifest;

use std::{fmt, str::FromStr};

use derivative::Derivative;
use petgraph::graph::NodeIndex;
use smol_str::SmolStr;
use thiserror::Error;

use crate::backend::Backend;

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

#[derive(Error, Debug)]
#[error("could not parse link time (expected \"buildtime\" or \"runtime\", found: {0:?})")]
pub struct LinkTimeParseError(String);

impl FromStr for LinkTime {
    type Err = LinkTimeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "buildtime" => Ok(LinkTime::Buildtime),
            "runtime" => Ok(LinkTime::Runtime),
            _ => Err(LinkTimeParseError(s.to_string())),
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

#[derive(Debug, Hash, PartialEq, Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DispatchRequest<B: Backend> {
    pub executor: SmolStr,
    pub payload: B::Value,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Dependency {
    pub node: NodeIndex,
    pub time: LinkTime,
}

#[derive(Debug, PartialEq, Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Package<B: Backend> {
    pub name: PackageName,
    pub metadata: Metadata,
    pub requests: Vec<DispatchRequest<B>>,
    pub dependencies: Vec<Dependency>,
}

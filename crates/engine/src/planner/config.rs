use std::sync::Arc;

use educe::Educe;
use log::trace;
use petgraph::graph::NodeIndex;
use xh_reports::prelude::*;

use crate::{
    backend::{Backend, Error as BackendError},
    name::PackageName,
    package::Package,
    planner::{Planner, Unfrozen},
    utils::passthru::PassthruHashMap,
};

#[derive(Debug, Default, IntoReport)]
#[message("could not configure packages")]
pub struct Error;

#[derive(Educe)]
#[educe(Debug, Clone(bound()))]
pub struct Config<B: Backend> {
    current: B::Value,
    #[educe(Debug(ignore))]
    apply: Arc<dyn Fn(B::Value) -> Result<Package, BackendError> + Send + Sync>,
}

impl<B: Backend> Config<B> {
    #[inline]
    pub fn new<F>(defaults: B::Value, apply: F) -> Self
    where
        F: Fn(B::Value) -> Result<Package, BackendError>,
        F: Send + Sync + 'static,
    {
        Config {
            current: defaults,
            apply: Arc::new(apply),
        }
    }

    pub fn apply(self) -> Result<Package, BackendError> {
        (self.apply)(self.current)
    }
}

pub struct ConfigManager<'a, B: Backend> {
    configs: PassthruHashMap<NodeIndex, Config<B>>,
    pub planner: &'a mut Planner<Unfrozen>,
}

impl<'a, B: Backend> ConfigManager<'a, B> {
    #[inline]
    pub fn new(planner: &'a mut Planner<Unfrozen>) -> Self {
        Self {
            planner,
            configs: Default::default(),
        }
    }

    #[inline]
    pub fn register(&mut self, name: PackageName, config: Config<B>) -> Result<(), Error> {
        let mut package = config.clone().apply().wrap()?;
        package.name = name;

        let node = self.planner.register(package).wrap()?;
        self.configs.insert(node, config);

        Ok(())
    }

    #[inline]
    pub fn configure(
        &mut self,
        source: &NodeIndex,
        destination: PackageName,
        modify: impl FnOnce(B::Value) -> Result<B::Value, BackendError>,
    ) -> Option<Result<(), Error>> {
        trace!("configuring from {source:?} into {destination}");

        self.configs.get(source).cloned().map(|source| {
            self.register(
                destination,
                Config {
                    current: modify(source.current).wrap()?,
                    apply: source.apply,
                },
            )
        })
    }
}

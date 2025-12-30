use std::{collections::HashMap, ops::Deref};

use crate::{
    backend::Backend,
    package::PackageName,
    planner::Plan,
    store::{ArtifactId, Store},
};

#[derive(Default)]
pub struct Manifest(HashMap<PackageName, ArtifactId>);

impl Deref for Manifest {
    type Target = HashMap<PackageName, ArtifactId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Manifest {
    async fn populate<'a, S: Store>(
        mut self,
        iter: impl Iterator<Item = &'a PackageName>,
        store: &S,
    ) -> Result<Self, S::Error> {
        for id in iter {
            store
                .package(id)
                .await
                .next()
                .transpose()?
                .and_then(|pkg| self.0.insert(pkg.package, pkg.artifact));
        }

        Ok(self)
    }

    pub async fn create<B: Backend, S: Store>(plan: &Plan<B>, store: &S) -> Result<Self, S::Error> {
        Self::default()
            .populate(plan.node_weights().map(|pkg| &pkg.name), store)
            .await
    }

    pub async fn update<S: Store>(self, store: &S) -> Result<Self, S::Error> {
        Self::default().populate(self.0.keys(), store).await
    }
}

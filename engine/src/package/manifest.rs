use std::{collections::HashMap, ops::Deref};

use crate::{
    package::PackageId,
    planner::Plan,
    store::{ArtifactId, Error as StoreError, Store},
};

#[derive(Default)]
pub struct Manifest(HashMap<PackageId, ArtifactId>);

impl Deref for Manifest {
    type Target = HashMap<PackageId, ArtifactId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Manifest {
    async fn populate<'a, S: Store>(
        mut self,
        iter: impl Iterator<Item = &'a PackageId>,
        store: &S,
    ) -> Result<Self, StoreError> {
        for id in iter {
            store
                .package(id)
                .await?
                .next()
                .and_then(|pkg| self.0.insert(pkg.package, pkg.artifact));
        }

        Ok(self)
    }

    pub async fn create<S: Store>(plan: &Plan, store: &S) -> Result<Self, StoreError> {
        Self::default()
            .populate(plan.node_weights().map(|pkg| &pkg.id), store)
            .await
    }

    pub async fn update<S: Store>(self, store: &S) -> Result<Self, StoreError> {
        Self::default().populate(self.0.keys(), store).await
    }
}

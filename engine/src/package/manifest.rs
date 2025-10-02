use std::{collections::HashMap, ops::Deref};

use crate::{
    planner::Plan,
    store::{ArtifactId, Error as StoreError, PackageId, Store},
};

pub struct Manifest(HashMap<PackageId, ArtifactId>);

fn make_manifest<'a, S: Store>(
    iter: impl Iterator<Item = PackageId>,
    store: &S,
) -> Result<Manifest, StoreError> {
    Ok(Manifest(
        iter.map(|id| {
            let pkg = store
                .packages(&id)?
                .next()
                .ok_or(StoreError::PackageNotFound(id.clone()))?;
            Ok((id, pkg.artifact))
        })
        .collect::<Result<_, StoreError>>()?,
    ))
}

impl Deref for Manifest {
    type Target = HashMap<PackageId, ArtifactId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Manifest {
    pub fn create<S: Store>(plan: &Plan, store: &S) -> Result<Self, StoreError> {
        make_manifest(plan.node_weights().map(|pkg| pkg.id.clone()), store)
    }

    pub fn update<S: Store>(self, store: &S) -> Result<Self, StoreError> {
        make_manifest(self.0.into_keys(), store)
    }
}

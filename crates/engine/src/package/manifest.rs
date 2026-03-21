use std::{borrow::Borrow, collections::HashMap, ops::Deref};

use crate::{
    backend::Backend,
    planner::{PackageId, Plan},
    store::{ArtifactId, Error, Store},
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
    async fn populate<S: Store>(
        self,
        _iter: impl IntoIterator<Item = impl Borrow<PackageId>>,
        _store: &S,
    ) -> Result<Self, Error> {
        todo!()
    }

    pub async fn create<B: Backend, S: Store>(_plan: &Plan, _store: &S) -> Result<Self, Error> {
        todo!()
    }

    pub async fn update<S: Store>(self, _store: &S) -> Result<Self, Error> {
        todo!()
    }
}

use std::{borrow::Borrow, collections::HashMap, ops::Deref};

use crate::{
    backend::Backend,
    planner::{PackageId, Plan},
    store::{ArtifactId, Store},
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
        self,
        iter: impl IntoIterator<Item = impl Borrow<PackageId>>,
        store: &S,
    ) -> Result<Self, S::Error> {
        todo!()
    }

    pub async fn create<B: Backend, S: Store>(plan: &Plan<B>, store: &S) -> Result<Self, S::Error> {
        todo!()
    }

    pub async fn update<S: Store>(self, store: &S) -> Result<Self, S::Error> {
        todo!()
    }
}

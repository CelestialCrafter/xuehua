use std::path::Path;

use thiserror::Error;

use crate::{
    package::PackageName,
    store::{ArtifactId, Store, StoreArtifact, StorePackage},
};

#[derive(Error, Debug)]
#[error("registry is unimplemented for EmptyStore")]
pub struct CannotRegister;

#[derive(Default, Clone, Copy)]
pub struct EmptyStore;

impl Store for EmptyStore {
    type Error = CannotRegister;

    async fn register_package(
        &mut self,
        _package: &PackageName,
        _artifact: &ArtifactId,
    ) -> Result<StorePackage, Self::Error> {
        Err(CannotRegister)
    }

    async fn package(
        &self,

        _package: &PackageName,
    ) -> impl Iterator<Item = Result<StorePackage, Self::Error>> {
        std::iter::empty()
    }

    async fn register_artifact(&mut self, _content: &Path) -> Result<StoreArtifact, Self::Error> {
        Err(CannotRegister)
    }

    async fn artifact(&self, _artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Self::Error> {
        Ok(None)
    }
}

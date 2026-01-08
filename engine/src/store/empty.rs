use thiserror::Error;
use xh_archive::Event;

use crate::{
    planner::PackageId,
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
        _package: &PackageId,
        _artifact: &ArtifactId,
    ) -> Result<StorePackage, Self::Error> {
        Err(CannotRegister)
    }

    async fn package(&self, _package: &PackageId) -> Result<Option<StorePackage>, Self::Error> {
        Ok(None)
    }

    async fn register_artifact(
        &mut self,
        _archive: Vec<Event>,
    ) -> Result<StoreArtifact, Self::Error> {
        Err(CannotRegister)
    }

    async fn artifact(&self, _artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Self::Error> {
        Ok(None)
    }

    async fn download(&self, _artifact: &ArtifactId) -> Result<Option<Vec<Event>>, Self::Error> {
        Ok(None)
    }
}

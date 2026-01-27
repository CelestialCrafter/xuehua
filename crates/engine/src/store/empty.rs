use xh_archive::Event;
use xh_reports::prelude::*;

use crate::{
    planner::PackageId,
    store::{ArtifactId, Error, Store, StoreArtifact, StorePackage},
};

#[derive(Debug, IntoReport)]
#[message("registry is unimplemented for EmptyStore")]
pub struct CannotRegister;

#[derive(Default, Clone, Copy)]
pub struct EmptyStore;

impl Store for EmptyStore {
    async fn register_package(
        &mut self,
        _package: &PackageId,
        _artifact: &ArtifactId,
    ) -> Result<StorePackage, Error> {
        Err(CannotRegister.wrap())
    }

    async fn package(&self, _package: &PackageId) -> Result<Option<StorePackage>, Error> {
        Ok(None)
    }

    async fn register_artifact(
        &mut self,
        _archive: Vec<Event>,
    ) -> Result<StoreArtifact, Error> {
        Err(CannotRegister.wrap())
    }

    async fn artifact(&self, _artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Error> {
        Ok(None)
    }

    async fn download(&self, _artifact: &ArtifactId) -> Result<Option<Vec<Event>>, Error> {
        Ok(None)
    }
}

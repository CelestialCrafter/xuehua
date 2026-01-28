use std::sync::LazyLock;

use xh_archive::Event;
use xh_reports::prelude::*;

use crate::{
    gen_name,
    name::StoreName,
    planner::PackageId,
    store::{ArtifactId, Error, Store, StoreArtifact, StorePackage},
};

#[derive(Debug, IntoReport)]
#[message("registry is unimplemented for EmptyStore")]
pub struct CannotRegister;

#[derive(Default, Clone, Copy)]
pub struct EmptyStore;

impl Store for EmptyStore {
    fn name() -> &'static StoreName {
        static NAME: LazyLock<StoreName> = LazyLock::new(|| gen_name!(empty@xuehua));
        &*NAME
    }

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

    async fn register_artifact(&mut self, _archive: Vec<Event>) -> Result<StoreArtifact, Error> {
        Err(CannotRegister.wrap())
    }

    async fn artifact(&self, _artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Error> {
        Ok(None)
    }

    async fn download(&self, _artifact: &ArtifactId) -> Result<Option<Vec<Event>>, Error> {
        Ok(None)
    }
}

use std::{
    iter::empty,
    path::{Path, PathBuf},
};

use crate::{
    package::{Package, PackageId},
    store::Store,
    utils::BoxDynError,
};

use crate::store::{ArtifactId, Error, StoreArtifact, StorePackage};

#[derive(Error, Debug)]
#[error("registry is unimplemented for EmptyStore")]
struct CannotRegister;

#[derive(Default, Clone, Copy)]
pub struct EmptyStore;

impl Store for EmptyStore {
    async fn register_package(
        &mut self,
        _package: &Package,
        _artifact: &ArtifactId,
    ) -> Result<PackageId, Error> {
        Err(BoxDynError::from(CannotRegister).into())
    }

    async fn packages(
        &self,
        _package: &PackageId,
    ) -> Result<impl Iterator<Item = StorePackage>, Error> {
        Ok(empty())
    }

    async fn register_artifact(&mut self, _content: &Path) -> Result<ArtifactId, Error> {
        Err(BoxDynError::from(CannotRegister).into())
    }

    async fn artifact(&self, _artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Error> {
        Ok(None)
    }

    async fn content(&self, _artifact: &ArtifactId) -> Result<Option<PathBuf>, Error> {
        Ok(None)
    }
}

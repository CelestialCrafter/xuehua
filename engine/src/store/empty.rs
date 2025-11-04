use std::{iter::Empty, path::Path};

use crate::{
    package::{Package, PackageId},
    store::Store,
    utils::BoxDynError,
};

use crate::store::{ArtifactId, Error, StoreArtifact, StorePackage};

#[derive(Error, Debug)]
#[error("registry is unimplemented for EmptyStore")]
struct CannotRegister;

#[derive(Error, Debug)]
#[error("unpacking is unimplemented for EmptyStore")]
struct CannotUnpack;

#[derive(Default, Clone, Copy)]
pub struct EmptyStore;

impl Store for EmptyStore {
    async fn register_package(
        &mut self,
        _package: &Package,
        _artifact: &ArtifactId,
    ) -> Result<StorePackage, Error> {
        Err(BoxDynError::from(CannotRegister).into())
    }

    async fn package(
        &self,
        package: &PackageId,
    ) -> Result<impl Iterator<Item = StorePackage>, Error> {
        Err::<Empty<_>, _>(Error::PackageNotFound(package.clone()))
    }

    async fn register_artifact(&mut self, _content: &Path) -> Result<StoreArtifact, Error> {
        Err(BoxDynError::from(CannotRegister).into())
    }

    async fn artifact(&self, artifact: &ArtifactId) -> Result<StoreArtifact, Error> {
        Err(Error::ArtifactNotFound(artifact.clone()))
    }

    async fn unpack(&self, _artifact: &ArtifactId, _output_directory: &Path) -> Result<(), Error> {
        Err(BoxDynError::from(CannotUnpack).into())
    }
}

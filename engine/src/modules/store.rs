pub mod local;

use std::{
    io,
    path::{Path, PathBuf},
};

use jiff::Timestamp;
use thiserror::Error;

use crate::package::Package;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    IoError(#[from] io::Error),
}

pub type ArtifactHash = blake3::Hash;
pub type PackageHash = u64;

#[derive(Debug)]
pub struct StorePackage {
    pub hash: PackageHash,
    pub artifact: ArtifactHash,
    pub created_at: Timestamp,
}

#[derive(Debug)]
pub struct StoreArtifact {
    pub hash: ArtifactHash,
    pub created_at: Timestamp,
}

pub trait Store {
    fn register_package(
        &mut self,
        package: &Package,
        artifact: &ArtifactHash,
    ) -> Result<StorePackage, StoreError>;
    fn register_artifact(&mut self, content: &Path) -> Result<StoreArtifact, StoreError>;

    fn package(&self, package: &Package) -> Result<Option<StorePackage>, StoreError>;
    fn artifact(&self, artifact: ArtifactHash) -> Result<Option<StoreArtifact>, StoreError>;

    fn artifact_by_package(&self, package: &Package) -> Result<Option<StoreArtifact>, StoreError> {
        Ok(match self.package(package)? {
            Some(store_pkg) => self.artifact(store_pkg.artifact)?,
            None => None,
        })
    }

    fn artifact_content(&self, artifact: &ArtifactHash) -> PathBuf;

    // TODO: artifact/package deletion
    // TODO: operation log actions
}

#[cfg(feature = "local-store")]
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
    #[error("package {0} not found")]
    PackageNotFound(PackageHash),
    #[error("artifact {0} not found")]
    ArtifactNotFound(ArtifactHash),
    #[cfg(feature = "local-store")]
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    IOError(#[from] io::Error),
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
    ) -> Result<PackageHash, StoreError>;
    fn package(&self, package: &Package) -> Result<StorePackage, StoreError>;

    fn register_artifact(&mut self, content: &Path) -> Result<ArtifactHash, StoreError>;
    fn artifact(&self, artifact: &ArtifactHash) -> Result<StoreArtifact, StoreError>;
    fn content(&self, artifact: &ArtifactHash) -> Result<PathBuf, StoreError>;

    // TODO: artifact/package deletion
    // TODO: operation log actions
}

#[cfg(feature = "local-store")]
pub mod local;

#[cfg(feature = "local-store")]
pub use local::LocalStore;

use std::{
    fs::File,
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use blake3::Hash;
use jiff::Timestamp;
use thiserror::Error;
use walkdir::WalkDir;

use crate::package;

#[derive(Error, Debug)]
pub enum Error {
    #[error("package {0} not found")]
    PackageNotFound(PackageId),
    #[error("artifact {0} not found")]
    ArtifactNotFound(ArtifactId),
    #[error(transparent)]
    ExternalError(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type ArtifactId = blake3::Hash;
pub type PackageId = package::id::Id;

#[derive(Debug)]
pub struct StorePackage {
    pub package: PackageId,
    pub artifact: ArtifactId,
    pub created_at: Timestamp,
}

#[derive(Debug)]
pub struct StoreArtifact {
    pub artifact: ArtifactId,
    pub created_at: Timestamp,
}

// TODO: add examples for store implementation and usage
/// Content-addressed append-only repository for packages and artifacts
///
/// # Implementation Guidelines
/// - Once something is registered into the store, its contents **must** never change.
/// - Stores **must** ensure that [`Self::register_package`] and [`Self::register_artifact`] are idempotent. Registering the same thing twice should be a no-op
/// - Stores **must** use directories for all content inputs and outputs. If contents need to be packed or unpacked (eg. downloading package contents over the network), the store needs to handle it.
/// - The returned ArtifactHash **must** be a secure hash of the contents. The [`hash_directory`] utility function can be used as the canonical implementation.
pub trait Store {
    fn register_package(
        &mut self,
        package: &package::Package,
        artifact: &ArtifactId,
    ) -> Result<PackageId, Error>;
    fn packages(
        &self,
        package: &PackageId,
    ) -> Result<impl Iterator<Item = StorePackage>, Error>;

    fn register_artifact(&mut self, content: &Path) -> Result<ArtifactId, Error>;
    fn artifact(&self, artifact: &ArtifactId) -> Result<StoreArtifact, Error>;
    fn content(&self, artifact: &ArtifactId) -> Result<PathBuf, Error>;
}

pub fn hash_directory(dir: &Path) -> io::Result<Hash> {
    let mut hasher = blake3::Hasher::new();
    let map_walkdir_err = |err: walkdir::Error| {
        let fallback = io::Error::new(io::ErrorKind::Other, err.to_string());
        err.into_io_error().unwrap_or(fallback)
    };

    for entry in WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(map_walkdir_err)?;
        let metadata = entry.metadata().map_err(map_walkdir_err)?;
        let file_type = entry.file_type();
        if file_type.is_file() {
            let path = entry.path();
            let stripped_path = path
                .strip_prefix(dir)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidFilename, err))?;

            hasher.write_all(
                &[
                    stripped_path.as_os_str().as_bytes(),
                    &metadata.permissions().mode().to_be_bytes(),
                    &metadata.gid().to_be_bytes(),
                    &metadata.uid().to_be_bytes(),
                    &metadata.len().to_be_bytes(),
                ]
                .concat(),
            )?;
            hasher.update_reader(&mut File::open(path)?)?;
        }
    }

    Ok(hasher.finalize())
}

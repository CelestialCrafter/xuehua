#[cfg(feature = "local-store")]
pub mod local;

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

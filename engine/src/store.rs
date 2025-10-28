pub mod empty;
#[cfg(feature = "local-store")]
pub mod local;

pub use empty::EmptyStore;
#[cfg(feature = "local-store")]
pub use local::LocalStore;

use std::path::Path;

use jiff::Timestamp;

use crate::package::PackageName;

pub type ArtifactId = blake3::Hash;

#[derive(Debug)]
pub struct StorePackage {
    pub package: PackageName,
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
    type Error: std::error::Error + Send + Sync;

    fn register_package(
        &mut self,
        package: &PackageName,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<StorePackage, Self::Error>> + Send;

    fn package(
        &self,
        package: &PackageName,
    ) -> impl Future<Output = impl Iterator<Item = Result<StorePackage, Self::Error>>> + Send;

    fn register_artifact(
        &mut self,
        content: &Path,
    ) -> impl Future<Output = Result<StoreArtifact, Self::Error>> + Send;

    fn artifact(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<Option<StoreArtifact>, Self::Error>> + Send;
}

pub(crate) fn hash_directory(dir: &Path) -> Result<ArtifactId, std::io::Error> {
    let mut hasher = blake3::Hasher::new();
    todo!();
    Ok(hasher.finalize())
}

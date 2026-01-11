pub mod empty;

pub use empty::EmptyStore;

use jiff::Timestamp;
use xh_archive::Event;

use crate::planner::PackageId;

pub type ArtifactId = blake3::Hash;

#[derive(Debug)]
pub struct StorePackage {
    pub id: PackageId,
    pub artifact: ArtifactId,
    pub created_at: Timestamp,
}

#[derive(Debug)]
pub struct StoreArtifact {
    pub id: ArtifactId,
    pub created_at: Timestamp,
}

// TODO: add examples for store implementation and usage
/// Content-addressed append-only repository for packages and artifacts
///
/// # Implementation Guidelines
/// - Once something is registered into the store, its contents **must** never change.
/// - Stores **must** ensure that [`Self::register_package`] and [`Self::register_artifact`] are idempotent.
pub trait Store {
    type Error: std::error::Error + Send + Sync;

    fn register_package(
        &mut self,
        package: &PackageId,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<StorePackage, Self::Error>> + Send;

    fn package(
        &self,
        package: &PackageId,
    ) -> impl Future<Output = Result<Option<StorePackage>, Self::Error>> + Send;

    fn register_artifact(
        &mut self,
        archive: Vec<Event>,
    ) -> impl Future<Output = Result<StoreArtifact, Self::Error>> + Send;

    fn artifact(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<Option<StoreArtifact>, Self::Error>> + Send;

    fn download(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<
        Output = Result<Option<Vec<Event>>, Self::Error>,
    > + Send;
}

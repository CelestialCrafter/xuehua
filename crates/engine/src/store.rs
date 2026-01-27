pub mod empty;

pub use empty::EmptyStore;

use jiff::Timestamp;
use xh_archive::Event;
use xh_reports::prelude::*;

use crate::planner::PackageId;

#[derive(Default, Debug, IntoReport)]
#[message("could not execute store action")]
pub struct Error;

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

pub trait Store {
    fn register_package(
        &mut self,
        package: &PackageId,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<StorePackage, Error>> + Send;

    fn package(
        &self,
        package: &PackageId,
    ) -> impl Future<Output = Result<Option<StorePackage>, Error>> + Send;

    fn register_artifact(
        &mut self,
        archive: Vec<Event>,
    ) -> impl Future<Output = Result<StoreArtifact, Error>> + Send;

    fn artifact(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<Option<StoreArtifact>, Error>> + Send;

    fn download(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<
        Output = Result<Option<Vec<Event>>, Error>,
    > + Send;
}

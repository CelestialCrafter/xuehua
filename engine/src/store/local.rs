use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::{
    ExternalResult,
    package::Package,
    store::{ArtifactHash, Error, PackageHash, Store, StoreArtifact, StorePackage, hash_directory},
    utils::ensure_dir,
};

const DATABASE_NAME: &str = "store.db";

struct Queries;

impl Queries {
    pub const REGISTER_ARTIFACT: &'static str = "INSERT INTO artifacts (hash, created_at) VALUES (:hash, :timestamp) ON CONFLICT DO NOTHING";
    pub const REGISTER_PACKAGE: &'static str = "INSERT INTO packages (hash, artifact, created_at) VALUES (:hash, :artifact, :timestamp) ON CONFLICT DO NOTHING";
    pub const GET_PACKAGE: &'static str = "SELECT * FROM packages WHERE hash IS :hash";
    pub const GET_ARTIFACT: &'static str = "SELECT * FROM artifacts WHERE hash IS :hash";
}

fn row_to_package(row: &Row<'_>) -> Result<StorePackage, rusqlite::Error> {
    Ok(StorePackage {
        hash: u64::from_be_bytes(row.get("hash")?),
        artifact: blake3::Hash::from_bytes(row.get("artifact")?),
        created_at: row.get("created_at")?,
    })
}

fn row_to_artifact(row: &Row<'_>) -> Result<StoreArtifact, rusqlite::Error> {
    Ok(StoreArtifact {
        hash: blake3::Hash::from_bytes(row.get("hash")?),
        created_at: row.get("created_at")?,
    })
}

/// A local store using SQLite as a database, and locally stored contents
pub struct LocalStore<'a> {
    root: &'a Path,
    db: Connection,
}

impl<'a> LocalStore<'a> {
    pub fn new(root: &'a Path) -> Result<Self, Error> {
        let db = Connection::open(root.join(DATABASE_NAME)).into_store_err()?;
        db.execute_batch(include_str!("local/initialize.sql"))
            .into_store_err()?;

        ensure_dir(&root.join("content")).into_store_err()?;
        Ok(Self { root, db })
    }

    fn artifact_path(&self, hash: &ArtifactHash) -> PathBuf {
        self.root.join("content").join(hash.to_hex().as_str())
    }
}

impl Store for LocalStore<'_> {
    fn register_package(
        &mut self,
        package: &Package,
        artifact: &blake3::Hash,
    ) -> Result<PackageHash, Error> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);
        let hash = hasher.finish();

        self.db
            .execute(
                Queries::REGISTER_PACKAGE,
                named_params! {
                    ":hash": hash.to_be_bytes(),
                    ":artifact": artifact.as_bytes(),
                    ":timestamp": Timestamp::now()
                },
            )
            .into_store_err()?;

        Ok(hash)
    }

    fn package(&self, package: &Package) -> Result<StorePackage, Error> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);
        let hash = hasher.finish();

        self.db
            .query_one(
                Queries::GET_PACKAGE,
                named_params! { ":hash": hash.to_be_bytes() },
                row_to_package,
            )
            .optional()
            .into_store_err()?
            .ok_or(Error::PackageNotFound(hash))
    }

    fn register_artifact(&mut self, content: &Path) -> Result<ArtifactHash, Error> {
        let hash = hash_directory(content).into_store_err()?;

        self.db
            .execute(
                Queries::REGISTER_ARTIFACT,
                named_params! { ":hash": hash.as_bytes(), ":timestamp": Timestamp::now() },
            )
            .into_store_err()?;

        let to = self.artifact_path(&hash);
        fs::rename(content, to).into_store_err()?;

        Ok(hash)
    }

    fn artifact(&self, hash: &ArtifactHash) -> Result<StoreArtifact, Error> {
        self.db
            .query_one(
                Queries::GET_ARTIFACT,
                named_params! { ":hash": hash.as_bytes() },
                row_to_artifact,
            )
            .optional()
            .into_store_err()?
            .ok_or(Error::ArtifactNotFound(*hash))
    }

    fn content(&self, artifact: &ArtifactHash) -> Result<PathBuf, Error> {
        let path = self.artifact_path(artifact);
        if !path.try_exists().into_store_err()? {
            return Err(Error::ArtifactNotFound(*artifact));
        }

        Ok(path)
    }
}

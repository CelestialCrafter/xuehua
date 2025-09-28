use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::{
    modules::store::{
        ArtifactHash, PackageHash, Store, StoreArtifact, StoreError, StorePackage, hash_directory,
    },
    package::Package,
    utils::ensure_dir,
};

const DATABASE_NAME: &str = "store.sqlite";

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

pub struct LocalStore<'a> {
    root: &'a Path,
    db: Connection,
}

impl<'a> LocalStore<'a> {
    pub fn new(root: &'a Path, in_memory: bool) -> Result<Self, StoreError> {
        let db = if in_memory {
            Connection::open_in_memory()
        } else {
            Connection::open(root.join(DATABASE_NAME))
        }?;

        ensure_dir(&root.join("content"))?;
        db.execute_batch(include_str!("local/initialize.sql"))?;
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
    ) -> Result<PackageHash, StoreError> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);
        let hash = hasher.finish();

        self.db.execute(
            Queries::REGISTER_PACKAGE,
            named_params! {
                ":hash": hash.to_be_bytes(),
                ":artifact": artifact.as_bytes(),
                ":timestamp": Timestamp::now()
            },
        )?;

        Ok(hash)
    }

    fn package(&self, package: &Package) -> Result<StorePackage, StoreError> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);
        let hash = hasher.finish();

        self.db
            .query_one(
                Queries::GET_PACKAGE,
                named_params! { ":hash": hash.to_be_bytes() },
                row_to_package,
            )
            .optional()?
            .ok_or(StoreError::PackageNotFound(hash))
    }

    fn register_artifact(&mut self, content: &Path) -> Result<ArtifactHash, StoreError> {
        let hash = hash_directory(content)?;

        self.db.execute(
            Queries::REGISTER_ARTIFACT,
            named_params! { ":hash": hash.as_bytes(), ":timestamp": Timestamp::now() },
        )?;

        let to = self.artifact_path(&hash);
        if !fs::exists(&to)? {
            fs::rename(content, to)?;
        }

        Ok(hash)
    }

    fn artifact(&self, hash: &ArtifactHash) -> Result<StoreArtifact, StoreError> {
        self.db
            .query_one(
                Queries::GET_ARTIFACT,
                named_params! { ":hash": hash.as_bytes() },
                row_to_artifact,
            )
            .optional()?
            .ok_or(StoreError::ArtifactNotFound(*hash))
    }

    fn content(&self, artifact: &ArtifactHash) -> Result<PathBuf, StoreError> {
        let path = self.artifact_path(artifact);
        if !path.try_exists()? {
            return Err(StoreError::ArtifactNotFound(*artifact));
        }

        Ok(path)
    }
}

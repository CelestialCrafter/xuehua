use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, named_params, types::FromSqlError};
use tokio::sync::Mutex;

use crate::{
    ExternalError, ExternalResult,
    package::Package,
    store::{ArtifactId, Error, PackageId, Store, StoreArtifact, StorePackage, hash_directory},
    utils::ensure_dir,
};

const DATABASE_NAME: &str = "store.db";

struct Queries;

impl Queries {
    pub const REGISTER_ARTIFACT: &'static str =
        "INSERT INTO artifacts (artifact, created_at) VALUES (:artifact, :created_at)";
    pub const REGISTER_PACKAGE: &'static str = "INSERT INTO packages (package, artifact, created_at) VALUES (:package, :artifact, :created_at)";
    pub const GET_PACKAGE: &'static str =
        "SELECT * FROM packages WHERE package IS :package ORDER BY created_at DESC";
    pub const GET_ARTIFACT: &'static str = "SELECT 1 FROM artifacts WHERE artifact IS :artifact";
}

/// A local store using SQLite as a database, and locally stored contents
pub struct LocalStore<'a> {
    root: &'a Path,
    db: Mutex<Connection>,
}

impl<'a> LocalStore<'a> {
    pub fn new(root: &'a Path) -> Result<Self, Error> {
        let db = Connection::open(root.join(DATABASE_NAME)).into_store_err()?;
        db.execute_batch(include_str!("local/initialize.sql"))
            .into_store_err()?;

        ensure_dir(&root.join("content")).into_store_err()?;
        Ok(Self {
            root,
            db: db.into(),
        })
    }

    fn artifact_path(&self, hash: &ArtifactId) -> PathBuf {
        self.root.join("content").join(hash.to_hex().as_str())
    }
}
impl Store for LocalStore<'_> {
    async fn register_package(
        &mut self,
        package: &Package,
        artifact: &ArtifactId,
    ) -> Result<PackageId, Error> {
        self.db
            .lock()
            .await
            .execute(
                Queries::REGISTER_PACKAGE,
                named_params! {
                    ":package": package.id.to_string(),
                    ":artifact": artifact.as_bytes(),
                    ":created_at": Timestamp::now()
                },
            )
            .map(|_| package.id.clone())
            .into_store_err()
    }

    async fn packages(&self, id: &PackageId) -> Result<impl Iterator<Item = StorePackage>, Error> {
        Ok(self
            .db
            .lock()
            .await
            .prepare_cached(Queries::GET_PACKAGE)
            .into_store_err()?
            .query_map(named_params! { ":package": id.to_string() }, |row| {
                Ok(StorePackage {
                    package: PackageId::from_str(&row.get::<_, String>("package")?)
                        .map_err(FromSqlError::other)?,
                    artifact: ArtifactId::from_bytes(row.get("artifact")?),
                    created_at: row.get("created_at")?,
                })
            })
            .into_store_err()?
            .collect::<Result<Vec<_>, rusqlite::Error>>()
            .into_store_err()?
            .into_iter())
    }

    async fn register_artifact(&mut self, content: &Path) -> Result<blake3::Hash, Error> {
        let hash = hash_directory(content).into_store_err()?;

        match self.db.lock().await.execute(
            Queries::REGISTER_ARTIFACT,
            named_params! {
                ":artifact": hash.as_bytes(),
                ":created_at": Timestamp::now()
            },
        ) {
            Ok(_) => fs::rename(content, self.artifact_path(&hash))
                .map(|_| hash)
                .into_store_err(),
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ffi::ErrorCode::ConstraintViolation,
                    ..
                },
                ..,
            )) => Ok(hash),
            Err(err) => Err(err.into_store_err()),
        }
    }

    async fn artifact(&self, id: &ArtifactId) -> Result<Option<StoreArtifact>, Error> {
        self.db
            .lock()
            .await
            .query_one(
                Queries::GET_ARTIFACT,
                named_params! { ":artifact": id.as_bytes() },
                |row| {
                    Ok(StoreArtifact {
                        artifact: ArtifactId::from_bytes(row.get("artifact")?),
                        created_at: row.get("created_at")?,
                    })
                },
            )
            .optional()
            .into_store_err()
    }

    async fn content(&self, artifact: &ArtifactId) -> Result<Option<PathBuf>, Error> {
        let path = self.artifact_path(artifact);
        Ok(if !path.try_exists().into_store_err()? {
            None
        } else {
            Some(path)
        })
    }
}

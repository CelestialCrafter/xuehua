use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, named_params, types::FromSqlError};

use crate::{
    ExternalResult,
    package::Package,
    store::{ArtifactId, Error, PackageId, Store, StoreArtifact, StorePackage, hash_directory},
    utils::ensure_dir,
};

const DATABASE_NAME: &str = "store.db";

struct Queries;

impl Queries {
    pub const REGISTER_ARTIFACT: &'static str = "INSERT INTO artifacts (artifact, created_at) VALUES (:artifact, :created_at) ON CONFLICT DO NOTHING";
    pub const REGISTER_PACKAGE: &'static str = "INSERT INTO packages (package, artifact, created_at) VALUES (:package, :artifact, :created_at) ON CONFLICT DO NOTHING";
    pub const GET_PACKAGE: &'static str = "SELECT * FROM packages WHERE package IS :package";
    pub const GET_ARTIFACT: &'static str = "SELECT * FROM artifacts WHERE artifact IS :artifact";
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

    fn artifact_path(&self, hash: &ArtifactId) -> PathBuf {
        self.root.join("content").join(hash.to_hex().as_str())
    }
}

impl Store for LocalStore<'_> {
    fn register_package(
        &mut self,
        package: &Package,
        artifact: &ArtifactId,
    ) -> Result<StorePackage, Error> {
        match self.package(&package.id) {
            Err(Error::PackageNotFound(_)) => {
                let store_package = StorePackage {
                    package: package.id.clone(),
                    artifact: *artifact,
                    created_at: Timestamp::now(),
                };

                self.db
                    .execute(
                        Queries::REGISTER_PACKAGE,
                        named_params! {
                            ":package": store_package.package.to_string(),
                            ":artifact": store_package.artifact.as_bytes(),
                            ":created_at": Timestamp::now()
                        },
                    )
                    .into_store_err()?;

                Ok(store_package)
            }
            result => result,
        }
    }

    fn package(&self, id: &PackageId) -> Result<StorePackage, Error> {
        self.db
            .query_one(
                Queries::GET_PACKAGE,
                named_params! { ":package": id.to_string() },
                |row| {
                    Ok(StorePackage {
                        package: PackageId::from_str(&row.get::<_, String>("package")?)
                            .map_err(FromSqlError::other)?,
                        artifact: ArtifactId::from_bytes(row.get("artifact")?),
                        created_at: row.get("created_at")?,
                    })
                },
            )
            .optional()
            .into_store_err()?
            .ok_or(Error::PackageNotFound(id.clone()))
    }

    fn register_artifact(&mut self, content: &Path) -> Result<StoreArtifact, Error> {
        let hash = hash_directory(content).into_store_err()?;
        match self.artifact(&hash) {
            Err(Error::ArtifactNotFound(_)) => {
                let store_artifact = StoreArtifact {
                    artifact: hash,
                    created_at: Timestamp::now(),
                };

                fs::rename(content, self.artifact_path(&store_artifact.artifact))
                    .into_store_err()?;
                self.db
                    .execute(
                        Queries::REGISTER_ARTIFACT,
                        named_params! {
                            ":artifact": store_artifact.artifact.as_bytes(),
                            ":created_at": store_artifact.created_at
                        },
                    )
                    .into_store_err()?;

                Ok(store_artifact)
            }
            result => result,
        }
    }

    fn artifact(&self, id: &ArtifactId) -> Result<StoreArtifact, Error> {
        self.db
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
            .into_store_err()?
            .ok_or(Error::ArtifactNotFound(*id))
    }

    fn content(&self, artifact: &ArtifactId) -> Result<PathBuf, Error> {
        let path = self.artifact_path(artifact);
        if !path.try_exists().into_store_err()? {
            return Err(Error::ArtifactNotFound(*artifact));
        }

        Ok(path)
    }
}

use std::{
    fs,
    ops::DerefMut,
    path::{Path, PathBuf},
};

use jiff::Timestamp;
use log::debug;
use rusqlite::{Connection, OptionalExtension, named_params};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    store::{ArtifactId, PackageName, Store, StoreArtifact, StorePackage},
    utils::ensure_dir,
};

const DATABASE_NAME: &str = "store.db";

struct Queries;

impl Queries {
    const REGISTER_ARTIFACT: &'static str =
        "INSERT INTO artifacts (artifact, created_at) VALUES (:artifact, :created_at)";
    const REGISTER_PACKAGE: &'static str = "INSERT INTO packages (package, artifact, created_at) VALUES (:package, :artifact, :created_at)";
    const GET_PACKAGE: &'static str =
        "SELECT * FROM packages WHERE package IS :package ORDER BY created_at DESC";
    const GET_ARTIFACT: &'static str = "SELECT 1 FROM artifacts WHERE artifact IS :artifact";
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SQLiteError(#[from] rusqlite::Error),
}

/// A local store using SQLite as a database, and locally stored contents
pub struct LocalStore<'a> {
    root: &'a Path,
    db: Mutex<Connection>,
}

impl<'a> LocalStore<'a> {
    pub fn new(root: &'a Path) -> Result<Self, Error> {
        let db = Connection::open(root.join(DATABASE_NAME))?;
        db.execute_batch(include_str!("local/initialize.sql"))?;

        ensure_dir(&root.join("content"))?;
        Ok(Self {
            root,
            db: db.into(),
        })
    }

    fn artifact_path(&self, hash: &ArtifactId) -> PathBuf {
        self.root.join("content").join(hash.to_hex().as_str())
    }

    async fn package_inner(
        connection: impl DerefMut<Target = Connection>,
        id: &PackageName,
    ) -> impl Iterator<Item = Result<StorePackage, Error>> {
        todo!();
        std::iter::empty()
        // let error = |err| {
        //     itertools::Either::Right(std::iter::once(Err::<StorePackage, _>(Error::SQLiteError(
        //         err,
        //     ))))
        // };

        // let mut query = match connection.prepare_cached(Queries::GET_PACKAGE) {
        //     Ok(v) => v,
        //     Err(err) => return error(err),
        // };

        // let iterator = match query.query_map(named_params! { ":package": id.to_string() }, |row| {
        //     Ok(StorePackage {
        //         package: PackageName::from_str(&row.get::<_, String>("package")?)
        //             .map_err(FromSqlError::other)?,
        //         artifact: ArtifactId::from_bytes(row.get("artifact")?),
        //         created_at: row.get("created_at")?,
        //     })
        // }) {
        //     Ok(v) => v,
        //     Err(err) => return error(err),
        // };

        // let iterator = iterator
        //     .map(|result| result.map_err(Into::into))
        //     .collect::<Vec<_>>()
        //     .into_iter();

        // itertools::Either::Left(iterator)
    }
}

impl Store for LocalStore<'_> {
    type Error = Error;

    async fn register_package(
        &mut self,
        package: &PackageName,
        artifact: &ArtifactId,
    ) -> Result<StorePackage, Self::Error> {
        debug!("registering package {} with artifact {}", package, artifact);

        let connection = self.db.lock().await;
        connection.execute(
            Queries::REGISTER_PACKAGE,
            named_params! {
                ":package": package.to_string(),
                ":artifact": artifact.as_bytes(),
                ":created_at": Timestamp::now()
            },
        )?;

        let package = Self::package_inner(connection, &package)
            .await
            .next()
            .ok_or(Error::SQLiteError(rusqlite::Error::QueryReturnedNoRows))??;

        Ok(package)
    }

    async fn package(
        &self,
        package: &PackageName,
    ) -> impl Iterator<Item = Result<StorePackage, Self::Error>> {
        Self::package_inner(self.db.lock().await, package)
            .await
            .collect::<Vec<_>>()
            .into_iter()
    }

    async fn register_artifact(&mut self, content: &Path) -> Result<StoreArtifact, Error> {
        let hash: blake3::Hash = todo!("hash archive");
        debug!("registering artifact {:?} as {}", content, hash);

        match self.db.lock().await.execute(
            Queries::REGISTER_ARTIFACT,
            named_params! {
                ":artifact": hash.as_bytes(),
                ":created_at": Timestamp::now()
            },
        ) {
            Ok(_) => fs::rename(content, self.artifact_path(&hash))?,
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ffi::ErrorCode::ConstraintViolation,
                    ..
                },
                ..,
            )) => (),
            Err(err) => return Err(err.into()),
        };

        self.artifact(&hash)
            .await?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    async fn artifact(&self, artifact: &ArtifactId) -> Result<Option<StoreArtifact>, Error> {
        self.db
            .lock()
            .await
            .query_one(
                Queries::GET_ARTIFACT,
                named_params! { ":artifact": artifact.as_bytes() },
                |row| {
                    Ok(StoreArtifact {
                        artifact: ArtifactId::from_bytes(row.get("artifact")?),
                        created_at: row.get("created_at")?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

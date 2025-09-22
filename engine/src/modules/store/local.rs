use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Row, Statement, named_params};
use walkdir::WalkDir;

use crate::{
    TEMP_DIR,
    modules::store::{ArtifactHash, Store, StoreArtifact, StoreError, StorePackage},
    package::Package,
};

const DATABASE_NAME: &str = "store.sqlite";

struct Queries;

impl Queries {
    pub const REGISTER_ARTIFACT: &'static str = "INSERT INTO artifacts (hash, created_at) VALUES (:hash, :timestamp) ON CONFLICT DO NOTHING";

    pub const REGISTER_PACKAGE: &'static str = "INSERT INTO packages (hash, artifact, created_at) VALUES (:hash, :artifact, :timestamp) ON CONFLICT DO NOTHING";

    pub const GET_ARTIFACT: &'static str = "SELECT * FROM packages WHERE hash IS :hash";

    pub const GET_PACKAGE: &'static str = "SELECT * FROM artifacts WHERE hash IS :hash";
}

pub struct LocalStore {
    root: PathBuf,
    db: Connection,
}

impl LocalStore {
    pub fn new(root: PathBuf, in_memory: bool) -> Result<Self, StoreError> {
        let db = if in_memory {
            Connection::open_in_memory()
        } else {
            Connection::open(root.join(DATABASE_NAME))
        }?;

        db.execute_batch(include_str!("local/initialize.sql"))?;
        Ok(Self { root, db })
    }
}

fn row_to_package(row: &Row<'_>) -> Result<StorePackage, rusqlite::Error> {
    Ok(StorePackage {
        hash: row.get("hash")?,
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

impl Store for LocalStore {
    fn register_package(
        &mut self,
        package: &Package,
        artifact: &blake3::Hash,
    ) -> Result<StorePackage, StoreError> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);

        Ok(self.db.query_one(
            Queries::REGISTER_PACKAGE,
            named_params! {
                ":hash": hasher.finish(),
                ":artifact": artifact.as_bytes(),
                ":timestamp": Timestamp::now()
            },
            row_to_package,
        )?)
    }

    fn register_artifact(&mut self, content: &Path) -> Result<StoreArtifact, StoreError> {
        let tmp_path = TEMP_DIR.join(format!(
            "temp-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should not be before the epoch")
                .as_millis()
        ));
        let mut tmp_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;

        let mut builder = tar::Builder::new(&mut tmp_file);
        for entry in WalkDir::new(content).sort_by_file_name().min_depth(1) {
            let entry = entry.map_err(|err| {
                let error = io::Error::new(io::ErrorKind::Other, err.to_string());
                err.into_io_error().unwrap_or(error)
            })?;
            let path = entry.path();
            builder.append_path(path)?;
        }
        builder.into_inner()?;

        let mut hasher = blake3::Hasher::new();
        hasher.update_reader(&mut tmp_file)?;
        let hash = hasher.finalize();

        let to = self.artifact_content(&hash);
        fs::rename(tmp_path, to)?;

        Ok(self.db.query_one(
            Queries::REGISTER_ARTIFACT,
            named_params! { ":hash": hash.as_bytes(), ":timestamp": Timestamp::now() },
            row_to_artifact,
        )?)
    }

    fn package(&self, package: &Package) -> Result<Option<StorePackage>, StoreError> {
        let hasher = &mut DefaultHasher::new();
        package.hash(hasher);

        Ok(self
            .db
            .query_one(
                Queries::GET_PACKAGE,
                named_params! { ":hash": hasher.finish() },
                row_to_package,
            )
            .optional()?)
    }

    fn artifact(&self, hash: ArtifactHash) -> Result<Option<StoreArtifact>, StoreError> {
        Ok(self
            .db
            .query_one(
                Queries::GET_ARTIFACT,
                named_params! { ":hash": hash.as_bytes() },
                row_to_artifact,
            )
            .optional()?)
    }

    fn artifact_content(&self, artifact: &blake3::Hash) -> PathBuf {
        self.root.join("content").join(artifact.to_hex().as_str())
    }
}

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use bytes::Bytes;
use derivative::Derivative;
use jiff::Timestamp;
use log::debug;
use rusqlite::{Connection, OptionalExtension, named_params};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use xh_archive::{Event, decoding::{Decoder, Error as DecodingError}};

use crate::{
    planner::PackageId,
    store::{ArtifactId, Store, StoreArtifact, StorePackage},
    utils::{ensure_dir, random_hash},
};

struct Queries;

impl Queries {
    const REGISTER_ARTIFACT: &'static str =
        "INSERT INTO artifacts (id, created_at) VALUES (:id, :created_at)";
    const REGISTER_PACKAGE: &'static str =
        "INSERT INTO packages (id, artifact, created_at) VALUES (:id, :artifact, :created_at)";
    const GET_PACKAGE: &'static str =
        "SELECT 1 FROM packages WHERE id IS :id ORDER BY created_at DESC";
    const GET_ARTIFACT: &'static str = "SELECT 1 FROM artifacts WHERE id IS :id";
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SQLiteError(#[from] rusqlite::Error),
    #[error(transparent)]
    DecodingError(#[from] DecodingError),
    #[error("could not communicate with task channel")]
    TaskError,
}

#[derive(Derivative)]
#[derivative(Debug)]
enum Task {
    RegisterPackage {
        package: PackageId,
        artifact: ArtifactId,
        #[derivative(Debug = "ignore")]
        channel: oneshot::Sender<Result<StorePackage, Error>>,
    },
    GetPackage {
        package: PackageId,
        #[derivative(Debug = "ignore")]
        channel: oneshot::Sender<Result<Option<StorePackage>, Error>>,
    },
    RegisterArtifact {
        #[derivative(Debug = "ignore")]
        archive: Vec<Event>,
        root: PathBuf,
        #[derivative(Debug = "ignore")]
        channel: oneshot::Sender<Result<StoreArtifact, Error>>,
    },
    GetArtifact {
        artifact: ArtifactId,
        #[derivative(Debug = "ignore")]
        channel: oneshot::Sender<Result<Option<StoreArtifact>, Error>>,
    },
    DecodeArtifact {
        artifact: ArtifactId,
        root: PathBuf,
        #[derivative(Debug = "ignore")]
        channel: oneshot::Sender<Result<Option<Vec<Event>>, Error>>,
    },
    Shutdown,
}

fn register_package(
    db: &mut Connection,
    package: PackageId,
    artifact: ArtifactId,
) -> Result<StorePackage, Error> {
    db.execute(
        Queries::REGISTER_PACKAGE,
        named_params! {
            ":id": package.as_bytes(),
            ":artifact": artifact.as_bytes(),
            ":created_at": Timestamp::now(),
        },
    )?;

    db.prepare_cached(Queries::GET_PACKAGE)?
        .query_one(named_params! { ":id": package.as_bytes() }, |row| {
            Ok(StorePackage {
                id: PackageId::from_bytes(row.get("id")?),
                artifact: ArtifactId::from_bytes(row.get("artifact")?),
                created_at: row.get("created_at")?,
            })
        })
        .optional()
        .transpose()
        .ok_or(Error::SQLiteError(rusqlite::Error::QueryReturnedNoRows))?
        .map_err(Into::into)
}

fn get_package(db: &mut Connection, package: PackageId) -> Result<Option<StorePackage>, Error> {
    db.prepare_cached(Queries::GET_PACKAGE)?
        .query_one(named_params! { ":id": package.as_bytes() }, |row| {
            Ok(StorePackage {
                id: PackageId::from_bytes(row.get("id")?),
                artifact: ArtifactId::from_bytes(row.get("artifact")?),
                created_at: row.get("created_at")?,
            })
        })
        .optional()
        .map_err(Into::into)
}

// TODO: reimplement this in a way that cant break in 200 different ways
fn register_artifact(
    db: &mut Connection,
    root: PathBuf,
    archive: Vec<Event>,
) -> Result<StoreArtifact, Error> {
    let temp = artifact_path(root.clone(), &random_hash());
    let file = File::create_new(&temp)?;

    let mut file = BufWriter::new(file);
    let mut buffer = bytes::BytesMut::with_capacity(1024 * 4);
    let mut encoder = xh_archive::encoding::Encoder::new();

    for event in &archive {
        buffer.clear();
        encoder.encode(&mut buffer, event);
        file.write_all(&buffer)?;
    }

    let digest = encoder.digest();
    std::fs::rename(temp, artifact_path(root, &digest))?;

    db.execute(
        Queries::REGISTER_ARTIFACT,
        named_params! {
            ":id": digest.as_bytes(),
            ":created_at": Timestamp::now()
        },
    )?;

    Ok(StoreArtifact {
        id: digest,
        created_at: Timestamp::now(),
    })
}

fn get_artifact(db: &mut Connection, artifact: ArtifactId) -> Result<Option<StoreArtifact>, Error> {
    db.query_one(
        Queries::GET_ARTIFACT,
        named_params! { ":id": artifact.as_bytes() },
        |row| {
            Ok(StoreArtifact {
                id: ArtifactId::from_bytes(row.get("id")?),
                created_at: row.get("created_at")?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn decode_artifact(root: PathBuf, artifact: ArtifactId) -> Result<Option<Vec<Event>>, Error> {
    let file = match File::open(artifact_path(root, &artifact)) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let mut mmap = Bytes::from_owner(unsafe { memmap2::Mmap::map(&file)? });
    let archive = Decoder::new()
        .decode_iter(&mut mmap)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(archive))
}

fn processing_thread(mut db: Connection, mut rx: mpsc::Receiver<Task>) {
    while let Some(task) = rx.blocking_recv() {
        debug!("processing task: {task:?}");

        match task {
            Task::RegisterPackage {
                package,
                artifact,
                channel,
            } => {
                let _ = channel.send(register_package(&mut db, package, artifact));
            }
            Task::GetPackage { package, channel } => {
                let _ = channel.send(get_package(&mut db, package));
            }
            Task::RegisterArtifact {
                archive,
                root,
                channel,
            } => {
                let _ = channel.send(register_artifact(&mut db, root, archive));
            }
            Task::GetArtifact { artifact, channel } => {
                let _ = channel.send(get_artifact(&mut db, artifact));
            }
            Task::DecodeArtifact {
                artifact,
                root,
                channel,
            } => {
                let _ = channel.send(decode_artifact(root, artifact));
            }
            Task::Shutdown => break,
        }
    }
}

/// A local store using SQLite as a database, and locally stored artifacts
pub struct LocalStore {
    tx: mpsc::Sender<Task>,
    root: PathBuf,
}

impl LocalStore {
    pub fn new(root: PathBuf) -> Result<Self, Error> {
        let root = root.join("artifacts");
        ensure_dir(&root)?;

        let db = Connection::open(root.join("store.db"))?;
        db.execute_batch(include_str!("local/initialize.sql"))?;

        let (tx, rx) = mpsc::channel(16);

        std::thread::Builder::new()
            .name("local-store-processor".to_string())
            .spawn(move || processing_thread(db, rx))?;

        Ok(Self { root, tx })
    }

    async fn queue<R>(
        &self,
        task: impl FnOnce(oneshot::Sender<Result<R, Error>>) -> Task,
    ) -> Result<R, Error> {
        let (req_tx, resp_rx) = oneshot::channel();

        self.tx
            .send(task(req_tx))
            .await
            .map_err(|_| Error::TaskError)?;

        resp_rx.await.map_err(|_| Error::TaskError).flatten()
    }
}

impl Store for LocalStore {
    type Error = Error;

    fn register_package(
        &mut self,
        package: &PackageId,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<StorePackage, Self::Error>> {
        self.queue(|channel| Task::RegisterPackage {
            package: *package,
            artifact: *artifact,
            channel,
        })
    }

    fn package(
        &self,
        package: &PackageId,
    ) -> impl Future<Output = Result<Option<StorePackage>, Self::Error>> {
        self.queue(|channel| Task::GetPackage {
            package: *package,
            channel,
        })
    }

    fn register_artifact(
        &mut self,
        archive: Vec<Event>,
    ) -> impl Future<Output = Result<StoreArtifact, Error>> {
        self.queue(|channel| Task::RegisterArtifact {
            archive,
            channel,
            root: self.root.clone(),
        })
    }

    fn artifact(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<Option<StoreArtifact>, Error>> {
        self.queue(|channel| Task::GetArtifact {
            artifact: *artifact,
            channel,
        })
    }

    fn download(
        &self,
        artifact: &ArtifactId,
    ) -> impl Future<Output = Result<Option<Vec<Event>>, Self::Error>> {
        self.queue(|channel| Task::DecodeArtifact {
            artifact: *artifact,
            root: self.root.clone(),
            channel,
        })
    }
}

impl Drop for LocalStore {
    fn drop(&mut self) {
        let _ = self.tx.send(Task::Shutdown);
    }
}

fn artifact_path(mut root: PathBuf, artifact: &ArtifactId) -> PathBuf {
    root.push(artifact.to_string());
    root
}

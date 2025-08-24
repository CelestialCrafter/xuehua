use std::{
    fs::{self, OpenOptions},
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

use blake3::Hash;
use eyre::{Report, Result, eyre};

#[derive(Debug)]
pub enum FetchError {
    CurlError(Report),
    IOError(Report),
    InvalidHash(Report),
}

impl Deref for FetchError {
    type Target = Report;

    fn deref(&self) -> &Self::Target {
        match self {
            FetchError::CurlError(err) => err,
            FetchError::IOError(err) => err,
            FetchError::InvalidHash(err) => err,
        }
    }
}

impl From<io::Error> for FetchError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value.into())
    }
}

pub struct FetchOptions<'a> {
    pub url: &'a str,
    pub hash: Hash,
    pub store: &'a Path,
    pub curl_opts: &'a [&'a str],
}

struct FileGuard {
    path: PathBuf,
    keep: bool,
}

impl FileGuard {
    fn new(path: impl Into<PathBuf>) -> Self {
        // TODO: lock path while in scope
        Self { path: path.into(), keep: false }
    }

    fn keep(mut self) -> PathBuf {
        self.keep = true;
        self.path.clone()
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        if self.keep {
            return;
        }

        if let Err(err) = fs::remove_file(&self.path) {
            eprintln!("could not remove {:?}: {err}", self.path);
        }
    }
}

impl Deref for FileGuard {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

// TODO: return a Package instead of a PathBuf
pub fn fetch(options: FetchOptions) -> Result<PathBuf, FetchError> {
    let path = options.store.join(options.hash.to_string());
    // TODO: verify path hash
    if fs::exists(&path)? {
        return Ok(path);
    }

    let path = FileGuard::new(path);

    duct::cmd(
        "curl",
        options
            .curl_opts
            .into_iter()
            .chain(["--", options.url].iter()),
    )
    .stdout_path(&*path)
    .run()
    .map_err(|err| FetchError::CurlError(err.into()))?;

    let mut file = OpenOptions::new().read(true).open(&*path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(&mut file)?;

    let real = hasher.finalize();
    let expected = options.hash;
    if expected != real {
        return Err(FetchError::InvalidHash(
            eyre!("expected hash did not match real hash")
                .wrap_err(format!("expected: {expected}\nreal: {real}")),
        ));
    }

    Ok(path.keep())
}

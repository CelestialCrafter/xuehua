use std::{
    fs::{self, OpenOptions},
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

use blake3::Hash;
use curl::easy::Easy;
use eyre::{Report, Result, eyre};
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum FetchError {
    #[error]
    CurlError(
        #[source]
        #[from]
        curl::Error,
    ),
    IOError(
        #[source]
        #[from]
        io::Error,
    ),
}

pub struct CurlOptions {}

pub struct Fetcher {
    curl: Easy,
    curl_opts: CurlOptions,
}

// TODO: return a Package instead of a PathBuf
impl Fetcher {
    pub fn fetch(&self, url: Url) -> Result<PathBuf, FetchError> {
        // TODO: verify path hash
        if fs::exists(&path)? {
            return Ok(path);
        }

        let path = FileGuard::new(path);

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
}

pub mod passthru;

use std::{fs, io, path::Path};

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

pub fn random_hash() -> blake3::Hash {
    let mut buffer = [0; blake3::OUT_LEN];
    fastrand::fill(&mut buffer);
    blake3::Hash::from_bytes(buffer)
}

pub fn ensure_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

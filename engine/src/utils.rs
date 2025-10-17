pub mod passthru;

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

#[inline]
pub fn random_hash() -> blake3::Hash {
    let mut buffer = [0; blake3::OUT_LEN];
    fastrand::fill(&mut buffer);
    blake3::Hash::from_bytes(buffer)
}

#[inline]
pub fn ensure_dir(path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

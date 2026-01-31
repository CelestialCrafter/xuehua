pub mod passthru;

#[inline]
pub fn ensure_dir(path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

pub mod passthru;

use std::{fs, io, path::Path};

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

pub fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(_) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(err) => Err(err),
    }
}

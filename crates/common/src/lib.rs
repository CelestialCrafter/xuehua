use std::path::{Component, Path, PathBuf};

use xh_reports::prelude::*;

pub mod serde_display;

#[derive(Debug, IntoReport)]
#[message("path escapes root")]
#[context(path, root)]
pub struct InvalidPathError {
    path: PathBuf,
    root: PathBuf,
}

pub fn safe_path(root: &Path, path: &Path) -> Result<PathBuf, InvalidPathError> {
    let resolved = path.components().fold(root.to_path_buf(), |mut acc, x| {
        match x {
            Component::Prefix(_) => acc.push(x),
            Component::RootDir => acc.push(x),
            Component::CurDir => (),
            Component::ParentDir => {
                acc.pop();
            }
            Component::Normal(_) => acc.push(x),
        }

        acc
    });

    resolved
        .starts_with(root)
        .then_some(resolved)
        .ok_or_else(|| {
            InvalidPathError {
                path: path.to_path_buf(),
                root: root.to_path_buf(),
            }
            .into_report()
        })
}

pub fn random_hash() -> blake3::Hash {
    let mut buffer = [0; blake3::OUT_LEN];
    fastrand::fill(&mut buffer);
    blake3::Hash::from_bytes(buffer)
}

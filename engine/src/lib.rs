use std::{env::temp_dir, fs::create_dir_all, path::PathBuf, sync::LazyLock};

pub(crate) static TEMP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = temp_dir().join("xuehua");
    create_dir_all(&path).expect("could not create tmp dir");
    path
});

pub mod modules;
pub mod package;

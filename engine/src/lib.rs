use std::{env::temp_dir, path::PathBuf, sync::LazyLock};

pub(crate) static TEMP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = temp_dir().join("xuehua");
    std::fs::create_dir_all(&path).expect("could not create tmp dir");
    path
});

pub mod utils;
pub mod modules;
pub mod package;

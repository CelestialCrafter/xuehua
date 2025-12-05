use std::path::Path;

use blake3::Hash;
use bytes::Bytes;

use crate::dictionary::{Error, DictionaryLoader};

#[derive(Clone, Copy, Default)]
pub enum Behavior {
    #[default]
    Read,
    #[cfg(feature = "mmap")]
    Mmap,
}

pub struct FilesystemLoader<'a> {
    path: &'a Path,
    behavior: Behavior,
}

impl<'a> FilesystemLoader<'a> {
    pub fn new(path: &'a Path, behavior: Behavior) -> Self {
        Self { path, behavior }
    }
}

impl DictionaryLoader for FilesystemLoader<'_> {
    fn load(&mut self, id: Hash) -> Result<Bytes, Error> {
        todo!()
    }
}

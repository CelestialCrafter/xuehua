use std::path::Path;

use blake3::Hash;
use bytes::Bytes;

use super::{Error, PrefixLoader};

pub struct FilesystemLoader<'a> {
    path: &'a Path,
}

impl<'a> FilesystemLoader<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path }
    }
}

impl PrefixLoader for FilesystemLoader<'_> {
    fn load(&mut self, id: Hash) -> Result<Bytes, Error> {
        todo!()
    }
}

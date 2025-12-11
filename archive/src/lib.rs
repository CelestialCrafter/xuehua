#![cfg_attr(not(feature = "std"), no_std)]

pub mod prefixes;
pub(crate) mod utils;

pub mod decoding;
pub mod encoding;

pub mod compression;
pub mod decompression;

#[cfg(all(feature = "std", unix))]
pub mod packing;
#[cfg(all(feature = "std", unix))]
pub mod unpacking;

extern crate alloc;

use alloc::collections::BTreeSet;

use blake3::Hash;
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathBytes {
    pub inner: Bytes,
}

impl From<Bytes> for PathBytes {
    fn from(value: Bytes) -> Self {
        Self { inner: value }
    }
}

#[cfg(all(feature = "std", unix))]
impl std::ops::Deref for PathBytes {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        let str: &std::ffi::OsStr = std::os::unix::ffi::OsStrExt::from_bytes(&self.inner);
        std::path::Path::new(str)
    }
}

#[cfg(all(feature = "std", unix))]
impl std::convert::AsRef<std::path::Path> for PathBytes {
    fn as_ref(&self) -> &std::path::Path {
        &*self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contents {
    Compressed(Bytes),
    Uncompressed(Bytes),
}

#[derive(Debug, Clone)]
pub enum Object {
    File { prefix: Option<Hash> },
    Symlink { target: PathBytes },
    Directory,
}

#[derive(Debug, Clone)]
pub enum Operation {
    Create { permissions: u32, object: Object },
    Delete,
}

#[derive(Debug, Clone)]
pub enum Event {
    Index(BTreeSet<PathBytes>),
    Operation(Operation),
    Contents(Contents),
}

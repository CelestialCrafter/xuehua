#![cfg_attr(not(feature = "std"), no_std)]

pub(crate) mod utils;

pub mod hashing;

pub mod decoding;
pub mod encoding;

#[cfg(all(feature = "std", unix))]
pub mod packing;
#[cfg(all(feature = "std", unix))]
pub mod unpacking;

extern crate alloc;

use alloc::collections::BTreeSet;
use core::fmt::Debug;

use bytes::Bytes;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathBytes {
    pub inner: Bytes,
}

impl Debug for PathBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
}

impl From<Bytes> for PathBytes {
    fn from(value: Bytes) -> Self {
        Self { inner: value }
    }
}

#[cfg(all(feature = "std", unix))]
impl std::convert::AsRef<std::path::Path> for PathBytes {
    fn as_ref(&self) -> &std::path::Path {
        let str: &std::ffi::OsStr = std::os::unix::ffi::OsStrExt::from_bytes(&self.inner);
        std::path::Path::new(str)
    }
}

#[cfg(all(feature = "std", unix))]
impl From<std::path::PathBuf> for PathBytes {
    fn from(value: std::path::PathBuf) -> Self {
        let bytes = std::os::unix::ffi::OsStringExt::into_vec(value.into_os_string());
        Bytes::from_owner(bytes).into()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Object {
    File {
        contents: Bytes,
    },
    Symlink {
        target: PathBytes,
    },
    Directory,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Operation {
    Create { permissions: u32, object: Object },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Index(BTreeSet<PathBytes>),
    Operation(Operation),
}

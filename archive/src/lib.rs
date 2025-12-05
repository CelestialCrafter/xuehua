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
use core::fmt::Debug;

use blake3::{Hash, Hasher};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contents {
    Compressed(Bytes),
    Decompressed(Bytes),
}

impl AsRef<Bytes> for Contents {
    fn as_ref(&self) -> &Bytes {
        match self {
            Contents::Compressed(bytes) => bytes,
            Contents::Decompressed(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Object {
    File {
        prefix: Option<Hash>,
        contents: Contents,
    },
    Symlink {
        target: PathBytes,
    },
    Directory,
}

impl Object {
    pub fn hash<'a>(&self, hasher: &'a mut Hasher) -> &'a mut Hasher {
        match self {
            Object::File {
                contents,
                prefix: _,
            } => {
                hasher.update(&[0]);
                utils::hash_plen(hasher, contents.as_ref())
            }
            Object::Symlink { target } => {
                hasher.update(&[1]);
                utils::hash_plen(hasher, &target.inner)
            }
            Object::Directory => hasher.update(&[2]),
        }
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::File {
                    contents: left,
                    prefix: _,
                },
                Self::File {
                    contents: right,
                    prefix: _,
                },
            ) => left == right,
            (Self::Symlink { target: left }, Self::Symlink { target: right }) => left == right,
            (Self::Directory, Self::Directory) => true,
            _ => false,
        }
    }
}

impl Eq for Object {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Operation {
    Create { permissions: u32, object: Object },
    Delete,
}

impl Operation {
    #[inline]
    pub fn hash<'a>(&self, hasher: &'a mut Hasher) -> &'a mut Hasher {
        match self {
            Operation::Create {
                permissions,
                object,
            } => {
                hasher.update(&[0]);
                hasher.update(&permissions.to_le_bytes());
                object.hash(hasher)
            }
            Operation::Delete => hasher.update(&[1]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Index(BTreeSet<PathBytes>),
    Operation(Operation),
}

#![cfg_attr(not(feature = "std"), no_std)]

pub mod dictionary;

pub mod decoding;
pub mod encoding;

#[cfg(feature = "std")]
pub mod packing;
#[cfg(feature = "std")]
pub mod unpacking;

extern crate alloc;

use alloc::collections::BTreeSet;
use core::fmt::Debug;

use blake3::Hasher;
use bytes::Bytes;

use crate::dictionary::Dictionary;

pub(crate) fn hash_plen<'a>(hasher: &'a mut Hasher, bytes: &Bytes) -> &'a mut Hasher {
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(&bytes)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathBytes {
    pub inner: Bytes,
}

impl Debug for PathBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contents {
    Compressed(Bytes),
    Uncompressed(Bytes),
}

impl AsRef<Bytes> for Contents {
    fn as_ref(&self) -> &Bytes {
        match self {
            Contents::Compressed(bytes) => bytes,
            Contents::Uncompressed(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Object {
    File {
        contents: Contents,
        dictionary: Dictionary,
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
                dictionary: _,
            } => {
                hasher.update(&[0]);
                hash_plen(hasher, contents.as_ref())
            }
            Object::Symlink { target } => {
                hasher.update(&[1]);
                hash_plen(hasher, &target.inner)
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
                    dictionary: _,
                },
                Self::File {
                    contents: right,
                    dictionary: _,
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

#[derive(Debug, Clone)]
pub enum Event {
    Index(BTreeSet<PathBytes>),
    Operation(Operation),
}

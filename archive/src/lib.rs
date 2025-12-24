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

use core::fmt::Debug;
use alloc::collections::BTreeMap;

use bytes::Bytes;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    EncodingError(#[from] encoding::Error),
    #[error(transparent)]
    DecodingError(#[from] decoding::Error),
    #[cfg(all(feature = "std", unix))]
    #[error(transparent)]
    PackingError(#[from] packing::Error),
    #[cfg(all(feature = "std", unix))]
    #[error(transparent)]
    UnpackingError(#[from] unpacking::Error),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectType {
    File,
    Symlink,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectMetadata {
    pub permissions: u32,
    pub size: u64,
    pub variant: ObjectType,
}

#[cfg(feature = "std")]
impl ObjectMetadata {
    #[inline]
    pub fn permissions(&self) -> std::fs::Permissions {
        std::os::unix::fs::PermissionsExt::from_mode(self.permissions)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Object {
    File { contents: Bytes },
    Symlink { target: PathBytes },
    Directory,
}

pub type Index = BTreeMap<PathBytes, ObjectMetadata>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Index(Index),
    Object(Object),
}

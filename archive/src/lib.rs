#![warn(missing_docs)]
#![warn(rustdoc::missing_doc_code_examples)]
#![cfg_attr(not(feature = "std"), no_std)]

//! # Xuehua Archive Format
//!
//! This crate provides an implementation of Xuehua Archives.
//!
//! Archives are represented as a sequence of [`Event`]s,
//! which can be processed by:
//! - [`decoding::Decoder`]: Decode from bytes
//! - [`encoding::Encoder`]: Encode into bytes
//! - [`hashing::Hasher`]: Hashing
//!
//! And with the `std` feature on `unix` targets:
//! - [`packing::Packer`]: Pack from the filesystem
//! - [`unpacking::Unpacker`]: Unpack into the filesystem
//!
#[doc = include_str!("../specification.md")]
pub(crate) mod utils;

pub mod hashing;

pub mod decoding;
pub mod encoding;

#[cfg(all(feature = "std", unix))]
pub mod packing;
#[cfg(all(feature = "std", unix))]
pub mod unpacking;

extern crate alloc;

use alloc::collections::BTreeMap;
use core::fmt::Debug;

use bytes::Bytes;
use thiserror::Error;

/// Root error type
#[derive(Error, Debug)]
pub enum Error {
    #[allow(missing_docs)]
    #[error(transparent)]
    EncodingError(#[from] encoding::Error),
    #[allow(missing_docs)]
    #[error(transparent)]
    DecodingError(#[from] decoding::Error),
    #[allow(missing_docs)]
    #[cfg(all(feature = "std", unix))]
    #[error(transparent)]
    PackingError(#[from] packing::Error),
    #[allow(missing_docs)]
    #[cfg(all(feature = "std", unix))]
    #[error(transparent)]
    UnpackingError(#[from] unpacking::Error),
}

/// A path internally represented with [`Bytes`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathBytes {
    inner: Bytes,
}

impl Debug for PathBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
}

impl From<PathBytes> for Bytes {
    fn from(value: PathBytes) -> Self {
        value.inner
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

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectType {
    #[allow(missing_docs)]
    File,
    #[allow(missing_docs)]
    Symlink,
    #[allow(missing_docs)]
    Directory,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectMetadata {
    pub permissions: u32,
    pub size: u64,
    pub variant: ObjectType,
}

#[cfg(feature = "std")]
impl ObjectMetadata {
    #[allow(missing_docs)]
    #[inline]
    pub fn permissions(&self) -> std::fs::Permissions {
        std::os::unix::fs::PermissionsExt::from_mode(self.permissions)
    }
}

/// An individual archive object
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Object {
    #[allow(missing_docs)]
    File { contents: Bytes },
    #[allow(missing_docs)]
    Symlink { target: PathBytes },
    #[allow(missing_docs)]
    Directory,
}

/// A map of paths to object metadata
pub type Index = BTreeMap<PathBytes, ObjectMetadata>;

/// An archive event.
///
/// An archive is represented as a sequence of [`Event`]s.
/// They must start with one [`Event::Index`], followed by [`Event::Object`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    #[allow(missing_docs)]
    Index(Index),
    #[allow(missing_docs)]
    Object(Object),
}

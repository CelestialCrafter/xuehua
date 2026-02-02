#![warn(missing_docs)]

//! # Xuehua Archive Format
//!
//! This crate provides an implementation of Xuehua Archives.
//!
//! Archives are represented as a sequence of [`Event`]s,
//! which can be processed by:
//! - [`decoding::Decoder`]: Decode from bytes
//! - [`encoding::Encoder`]: Encode into bytes
//!
//! And on `unix` targets:
//! - [`packing::Packer`]: Pack from the filesystem
//! - [`unpacking::Unpacker`]: Unpack into the filesystem
//!
#[doc = include_str!("../specification.md")]
pub(crate) mod utils;

pub mod decoding;
pub mod encoding;

#[cfg(unix)]
pub mod packing;
#[cfg(unix)]
pub mod unpacking;

use std::{
    fmt,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use ed25519_dalek::Signature;

/// A path internally represented with [`Bytes`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathBytes {
    inner: Bytes,
}

impl fmt::Debug for PathBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

#[cfg(unix)]
impl AsRef<Path> for PathBytes {
    fn as_ref(&self) -> &Path {
        let str: &std::ffi::OsStr = std::os::unix::ffi::OsStrExt::from_bytes(&self.inner);
        Path::new(str)
    }
}

#[cfg(unix)]
impl From<PathBuf> for PathBytes {
    fn from(value: PathBuf) -> Self {
        let bytes = std::os::unix::ffi::OsStringExt::into_vec(value.into_os_string());
        Bytes::from_owner(bytes).into()
    }
}

/// The contents of an object.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ObjectContent {
    #[allow(missing_docs)]
    File { data: Bytes },
    #[allow(missing_docs)]
    Symlink { target: PathBytes },
    #[allow(missing_docs)]
    Directory,
}

/// An individual file object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    #[allow(missing_docs)]
    pub location: PathBytes,
    #[allow(missing_docs)]
    pub permissions: u32,
    #[allow(missing_docs)]
    pub content: ObjectContent,
}

impl Object {
    #[allow(missing_docs)]
    #[inline]
    pub fn permissions(&self) -> std::fs::Permissions {
        std::os::unix::fs::PermissionsExt::from_mode(self.permissions)
    }
}

/// The fingerprint of a public key
pub type Fingerprint = blake3::Hash;

/// An individual archive event.
///
/// An archive is represented as a sequence of [`Event`]s.
/// They must start with one [`Event::Header`], followed by [`Event::Object`]s, and then finally an [`Event::Footer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The header containing the magic bytes, and the version.
    Header,
    /// An object containing an individual file
    Object(Object),
    /// The footer containing the archive digest and signature
    Footer(Vec<(Fingerprint, Signature)>),
}

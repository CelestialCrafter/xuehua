// TODO: include tests against `nix nar` in fs packing & unpacking

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]
//!
//! ## Examples
//!
//! Encode then decode a stream of NAR [`Events`](crate::Event)
//!
//! ```rust
//! use nix_archive::{decoding::Decoder, encoding::Encoder, Event};
//!
//! let content = "hello world!";
//! let events = vec![
//!     Event::Header,
//!     Event::Directory,
//!     Event::DirectoryEntry {
//!         name: "my-file".as_bytes().into(),
//!     },
//!     Event::Regular {
//!         executable: true,
//!         size: content.len() as u64,
//!     },
//!     Event::RegularContentChunk(content.as_bytes().into()),
//!     Event::DirectoryEnd,
//! ];
//!
//! // first we encode our events into a buffer
//! let mut encoded = bytes::BytesMut::new();
//! Encoder::new().encode(&mut encoded, &events)?;
//!
//! // and next we decode the buffer back into a list of events!
//! let decoded = Decoder::new()
//!     .decode(&mut encoded.freeze())
//!     .collect::<Result<Vec<_>, _>>()?;
//!
//! assert_eq!(events, decoded);
//! # Ok::<_, anyhow::Error>(())
//! ```

extern crate alloc;

pub mod decoding;
pub mod encoding;
#[cfg(all(feature = "std", unix))]
pub mod unpacking;
pub(crate) mod utils;
pub mod validation;

use bytes::Bytes;

/// An intermediate event type to describe a NAR file.
///
/// This enum loosely describes objects in the [specification](https://nix.dev/manual/nix/2.25/protocols/nix-archive),
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// NAR header ("nix-archive-1")
    Header,
    /// Regular file object
    ///
    /// Events after this must be [`Event::RegularContentChunk`]'s until the aggregate chunk length matches `size`.
    /// There also must be at least one [`Event::RegularContentChunk`]
    Regular {
        /// Whether the file is executable or not
        executable: bool,
        /// The size of the file
        size: u64,
    },
    /// A chunk of data corresponding to a Regular object
    RegularContentChunk(Bytes),
    /// A symlink file object
    Symlink {
        /// The target of the symlink
        target: Bytes,
    },
    /// The start of a directory object
    ///
    /// Only DirectoryEntry or DirectoryEnd events are allowed here
    Directory,
    /// An entry in the directory
    ///
    /// The next event must be an object
    DirectoryEntry {
        /// The basename of the entry
        name: Bytes,
    },
    /// The end of a directory object
    DirectoryEnd,
}

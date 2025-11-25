// TODO: include testing blobs in src control
// TODO: include tests against `nix nar` in fs packing & unpacking

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]
//! ## Caveats
//!
//! - Streaming only, no full encoding or decoding will ever exist in this crate
//! - Unix only\
//!     Doing things like checking if files are executable or handling symlinks
//!     are more difficult on windows, and would most likely be handled better
//!     via external dependencies, which I don't want to add.
//!
//! ## Examples
//!
//! Encode then decode a stream of NAR [`Event`](crate::validation::Event)
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
//! # #[derive(thiserror::Error, Debug)]
//! # enum Error {
//! #      #[error(transparent)]
//! #      DecodeError(#[from] nix_archive::decoding::Error),
//! #      #[error(transparent)]
//! #      EncodeError(#[from] nix_archive::encoding::Error)
//! # }
//!
//! // first we encode our events into a buffer (or anything else that impl Write)
//! let mut encoded = Vec::new();
//! Encoder::new(&mut encoded).encode_all(&events)?;
//!
//! // and next we decode the buffer (or anything else that impl Read) back into a list of events!
//! let decoded = Decoder::new().decode_all(&mut encoded.into()).collect::<Result<Vec<_>, _>>()?;
//!
//! assert_eq!(events, decoded, "decoded events did not match original");
//! # Ok::<_, Error>(())
//! ```

#[cfg(test)]
pub(crate) mod arbitrary;
pub mod decoding;
pub mod encoding;
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

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbtest::arbtest;
    use bytes::{Bytes, BytesMut};

    use crate::{
        Event,
        arbitrary::ArbitraryNar,
        decoding::Decoder,
        encoding::Encoder,
        utils::{TestingLogger, debug, info},
    };

    // collapses multiple chunk events so comparing equality between
    // semantically equivalent event streams doesn't error
    fn chunk_collapse(events: Vec<Event>) -> Vec<Event> {
        let length = events.len();
        events
            .into_iter()
            .fold(Vec::with_capacity(length), |mut acc, mut event| {
                if let Event::RegularContentChunk(ref mut chunk) = event {
                    acc.pop_if(|parent| match parent {
                        Event::RegularContentChunk(parent) => {
                            let mut bytes = BytesMut::new();
                            bytes.extend_from_slice(parent);
                            bytes.extend_from_slice(&chunk);
                            *chunk = bytes.freeze();
                            true
                        }
                        _ => false,
                    });
                }

                acc.push(event);
                acc
            })
    }

    fn decode(contents: &[u8]) -> Vec<Event> {
        let decoded = chunk_collapse(
            Decoder::new()
                .decode_all(&mut Bytes::copy_from_slice(contents))
                .collect::<Result<Vec<_>, _>>()
                .expect("decoding from bytes should not fail"),
        );

        debug!("decoder output: {decoded:?}");
        decoded
    }

    fn test_roundtrip_blob(contents: &'static [u8]) {
        TestingLogger::init();

        let mut encoded = Vec::new();
        Encoder::new(&mut encoded)
            .encode_all(decode(contents))
            .expect("encoding should not fail");
        let encoded = Bytes::from_owner(encoded);

        debug!("encoder output: {encoded:?}");
        assert_eq!(
            contents, encoded,
            "original events does not match decoded events"
        );
    }

    #[test]
    fn test_roundtrip_blob_rust_compiler() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-compiler.nar"));
    }

    #[test]
    fn test_roundtrip_blob_rust_core() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-core.nar"));
    }

    #[test]
    fn test_roundtrip_blob_rust_std() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-std.nar"));
    }

    #[test]
    fn arbtest_roundtrip() {
        TestingLogger::init();

        arbtest(|u| {
            let nar = ArbitraryNar::arbitrary(u)?;
            let events = chunk_collapse(nar.0);
            info!("event stream: {:#?}", events);

            let mut encoded = Vec::new();
            Encoder::new(&mut encoded)
                .encode_all(events.iter())
                .expect("encoding should not fail");
            let encoded = Bytes::from_owner(encoded);

            debug!("encoder output: {encoded:?}");
            assert!(
                events == decode(&encoded),
                "original events does not match decoded events"
            );

            Ok(())
        })
        .run()
    }
}

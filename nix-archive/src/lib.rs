// TODO: maybe include testing blobs in src control
// TODO: think of an ergonomic way to do zero-alloc coding

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
//! - Only UTF-8 encodable paths are supported\
//!     This shouldn't be an issue because the crate is unix-only, but
//!     the rationale for this is from [`OsStr::as_encoded_bytes`](std::ffi::OsStr::as_encoded_bytes):
//!     "As the encoding is unspecified, any sub-slice of bytes that is not valid UTF-8
//!     should be treated as opaque and only comparable within the same Rust version
//!     built for the same target platform."
//!
//! ## Examples
//!
//! Encode then decode a stream of NAR [`Event`](crate::state::Event)
//!
//! ```rust
//! use nix_archive::{decoding::Decoder, encoding::Encoder, state::Event};
//!
//! let content = "hello world!";
//! let events = vec![
//!     Event::Header,
//!     Event::Directory,
//!     Event::DirectoryEntry {
//!         name: std::ffi::OsString::from("my-file"),
//!     },
//!     Event::Regular {
//!         executable: true,
//!         size: content.len() as u64,
//!     },
//!     Event::RegularContentChunk(content.as_bytes().to_vec()),
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
//! Encoder::new(&mut encoded).copy(events.iter())?;
//!
//! // and next we decode the buffer (or anything else that impl Read) back into a list of events!
//! let decoded = Decoder::new(encoded.as_slice())
//!     .collect::<Result<Vec<_>, _>>()?;
//!
//! assert_eq!(events, decoded, "decoded events did not match original");
//! # Ok::<_, Error>(())
//! ```

pub mod state;
mod filesystem;
pub use filesystem::*;
mod coder;
pub use coder::*;

#[allow(dead_code)]
#[allow(unused_macros)]
#[allow(unused_imports)]
pub(crate) mod utils;

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbtest::arbtest;

    use crate::{
        coder::decoding::Decoder,
        coder::encoding::Encoder,
        state::{Event, arbitrary::ArbitraryNar},
        utils::log::{TestingLogger, info},
    };

    // collapses multiple chunk events so comparing equality between
    // semantically (but not technically) equivalent event streams doesn't error
    fn chunk_collapse(events: Vec<Event>) -> Vec<Event> {
        let length = events.len();
        events
            .into_iter()
            .fold(Vec::with_capacity(length), |mut acc, event| {
                if let Some(Event::RegularContentChunk(parent)) = acc.last_mut() {
                    if let Event::RegularContentChunk(chunk) = event {
                        parent.extend(chunk);
                        return acc;
                    }
                }

                acc.push(event);
                acc
            })
    }

    fn test_roundtrip_blob(contents: &[u8]) {
        TestingLogger::init();

        info!("blob contents: {:?}", contents);

        let decoded = Decoder::new(contents)
            .collect::<Result<Vec<_>, _>>()
            .expect("decoding should not fail");
        let events = chunk_collapse(decoded);
        info!("decoder output: {:#?}", decoded);

        let mut encoded = Vec::new();
        Encoder::new(&mut encoded)
            .copy(events.iter())
            .expect("encoding should not fail");
        info!("encoder output: {:?}", encoded);

        // not using assert_eq because the decoded events are logged above
        assert!(
            contents == encoded,
            "original events did not match decoded events"
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
                .copy(events.iter())
                .expect("encoding should not fail");
            info!("encoder output: {:?}", encoded);

            let decoded = Decoder::new(encoded.as_slice())
                .collect::<Result<Vec<_>, _>>()
                .expect("decoding should not fail");
            let decoded = chunk_collapse(decoded);
            info!("decoder output: {:#?}", decoded);

            // not using assert_eq because the decoded events are logged above
            assert!(
                events == decoded,
                "original events did not match decoded events"
            );

            Ok(())
        })
        .run()
    }
}

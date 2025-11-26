//! Encoder for NAR data streams
//!
//! # Examples
//!
//! Encoding NAR events to a NAR file on stdout
//!
//! ```rust
//! use nix_archive::{encoding::Encoder, Event};
//! use std::io::Write;
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
//! #      EncodeError(#[from] nix_archive::encoding::Error),
//! #      #[error(transparent)]
//! #      IOError(#[from] std::io::Error)
//! # }
//!
//! let mut encoded = bytes::BytesMut::new();
//! Encoder::new().encode_all(&mut encoded, events)?;
//!
//! std::io::stdout().write_all(&encoded)?;
//!
//! # Ok::<_, Error>(())
//! ```

use std::borrow::Borrow;

use bytes::{BufMut, BytesMut};
use thiserror::Error;

use crate::{
    Event,
    utils::{calculate_padding, trace},
    validation::{Error as ValidationError, EventValidator, StackFrame},
};

/// Error type for the [Encoder]
#[derive(Error, Debug)]
pub enum Error {
    /// The internal state errored
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}

/// Encodes NAR [Events](Event) into bytes
#[derive(Debug)]
pub struct Encoder {
    validator: EventValidator,
}

impl Encoder {
    /// Constructs a new [`Encoder`]
    #[inline]
    pub fn new() -> Self {
        Self {
            validator: EventValidator::new(),
        }
    }

    /// Encodes an iterator of [`Events`](Event) into an instance of [`BytesMut`]
    #[inline]
    pub fn encode_all<I: IntoIterator<Item = impl Borrow<Event>>>(
        &mut self,
        buffer: &mut BytesMut,
        iterator: I,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.encode(buffer, event.borrow()))
    }

    /// Encodes an [`Event`] into an instance of [`BytesMut`]
    #[inline]
    pub fn encode(&mut self, buffer: &mut BytesMut, event: &Event) -> Result<(), Error> {
        let attempt_len = buffer.len();
        let mut attempt_validator = self.validator.clone();
        match encode(&mut attempt_validator, buffer, event) {
            Ok(event) => {
                self.validator = attempt_validator;
                Ok(event)
            }
            Err(err) => {
                buffer.truncate(attempt_len);
                Err(err)
            }
        }
    }
}

#[inline]
fn encode(
    validator: &mut EventValidator,
    buffer: &mut BytesMut,
    event: &Event,
) -> Result<(), Error> {
    match event {
        Event::Header => string(buffer, "nix-archive-1"),
        Event::Regular { executable, size } => {
            string(buffer, "(");
            string(buffer, "type");
            string(buffer, "regular");

            if *executable {
                string(buffer, "executable");
                string(buffer, "");
            }

            string(buffer, "contents");
            buffer.put_u64_le(*size);
        }
        Event::RegularContentChunk(chunk) => buffer.put_slice(&chunk),
        Event::Symlink { target } => {
            string(buffer, "(");
            string(buffer, "type");
            string(buffer, "symlink");
            string(buffer, "target");
            string(buffer, target);
        }
        Event::Directory => {
            string(buffer, "(");
            string(buffer, "type");
            string(buffer, "directory");
        }
        Event::DirectoryEntry { name } => {
            string(buffer, "entry");
            string(buffer, "(");
            string(buffer, "name");
            string(buffer, name);
            string(buffer, "node");
        }
        Event::DirectoryEnd => (),
    }

    for deconstructed in validator.advance(&event)? {
        match deconstructed {
            StackFrame::Object | StackFrame::DirectoryEntry => string(buffer, ")"),
            StackFrame::RegularData { expected, .. } => padding(buffer, expected),
            _ => (),
        }
    }

    Ok(())
}

#[inline]
fn padding(buffer: &mut BytesMut, strlen: u64) {
    let padding = calculate_padding(strlen);
    trace!("writing {padding} bytes of padding");
    buffer.put_bytes(0, padding);
}

#[inline]
fn string(buffer: &mut BytesMut, value: impl AsRef<[u8]>) {
    let data = value.as_ref();
    let len = data.len() as u64;

    trace!(
        "writing string {:?} of size {len}",
        String::from_utf8_lossy(data)
    );

    buffer.put_u64_le(len);
    buffer.put_slice(data.as_ref());
    padding(buffer, len);
}

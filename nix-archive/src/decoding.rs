//! Decodes bytes into a NAR [`Event`] stream
//!
//! # Examples
//!
//! Decoding a NAR file from stdin into events on stderr
//!
//! ```rust,no_run
//! use nix_archive::decoding::Decoder;
//! use std::io::{Read, stdin};
//!
//! let mut buffer = Vec::new();
//! stdin().read_to_end(&mut buffer)?;
//!
//! for event in Decoder::new().decode_all(&mut buffer.into()) {
//!     eprintln!("{:?}", event?);
//! }
//!
//! # Ok::<_, anyhow::Error>(())
//! ```

use std::{fmt::Debug, num::TryFromIntError};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    Event,
    utils::{calculate_padding, debug, trace},
    validation::{Error as ValidationError, EventValidator, StackFrame},
};

/// Error type for the [`Decoder`]
#[derive(Error, Debug)]
pub enum Error {
    /// The underlying had unexpected non-NAR data
    #[error("unexpected token (expected {expected:?}, found {found:?})")]
    UnexpectedToken {
        /// A description of the expected token
        expected: String,
        /// The token that was read
        found: Bytes,
    },
    /// The provided data was not enough to form a complete [`Event`]
    #[error("input does not contain enough data")]
    Incomplete {
        /// The amount of bytes needed to continue decoding
        needed: usize
    },
    /// A number was too big to be converted
    #[error(transparent)]
    ConversionError(#[from] TryFromIntError),
    /// The internal event validator errored
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}

/// Decodes bytes into NAR [`Events`](Event)
#[derive(Debug)]
pub struct Decoder {
    validator: EventValidator,
}

impl Decoder {
    /// Constructs a new [`Decoder`]
    #[inline]
    pub fn new() -> Self {
        Self {
            validator: EventValidator::new(),
        }
    }

    /// Decodes an instance of [`Bytes`] into an [`Event`] stream
    ///
    /// On every successful decode, `bytes` will be shifted by the
    /// amount of data consumed. The return [`Events`](Event) borrow
    /// from this data.
    ///
    /// # Errors
    ///
    /// If this function encounters an error, both the internal state and
    /// `bytes` are rolled back, and further operations can be executed safely.
    ///
    /// If this function does not have enough data to decode an event, an
    /// [`Error::Incomplete`] is returned.
    #[inline]
    pub fn decode_all(&mut self, bytes: &mut Bytes) -> impl Iterator<Item = Result<Event, Error>> {
        std::iter::from_fn(|| {
            if self.validator.finished() {
                return None;
            }

            let mut attempt = (self.validator.clone(), bytes.clone());
            match decode(&mut attempt.0, &mut attempt.1) {
                Ok(event) => {
                    self.validator = attempt.0;
                    *bytes = attempt.1;

                    Some(Ok(event))
                }
                Err(err) => Some(Err(err)),
            }
        })
    }
}

fn decode(validator: &mut EventValidator, data: &mut Bytes) -> Result<Event, Error> {
    let frame = validator.peek()?;
    debug!("decoding event with {frame:?} context frame");

    let event = match frame {
        StackFrame::Header => {
            expect(data, "nix-archive-1")?;
            Event::Header
        }
        StackFrame::Object => {
            expect(data, "(")?;
            expect(data, "type")?;

            let ty = string(data)?;
            match ty.as_ref() {
                b"regular" => {
                    let executable = with_peeked_string(
                        data,
                        |str| str == b"executable",
                        |data| {
                            expect(data, "")?;
                            Ok::<_, Error>(true)
                        },
                        |_| Ok(false),
                    )??;

                    expect(data, "contents")?;

                    Event::Regular {
                        executable,
                        size: integer(data)?,
                    }
                }
                b"symlink" => {
                    expect(data, "target")?;
                    Event::Symlink {
                        target: string(data)?,
                    }
                }
                b"directory" => Event::Directory,
                _ => {
                    return Err(Error::UnexpectedToken {
                        expected: r#""regular", "symlink", or "directory""#.to_string(),
                        found: ty,
                    });
                }
            }
        }
        StackFrame::Directory => with_peeked_string(
            data,
            |str| str == b"entry",
            |data| {
                expect(data, "(")?;
                expect(data, "name")?;
                let name = string(data)?;
                expect(data, "node")?;

                Ok::<_, Error>(Event::DirectoryEntry { name })
            },
            |_| Ok(Event::DirectoryEnd),
        )??,
        StackFrame::DirectoryEntry => unreachable!("peeked frame was directory entry"),
        StackFrame::RegularData { expected, written } => {
            let size = (expected - written).min(data.len() as u64) as usize;
            Event::RegularContentChunk(data.split_to(size))
        }
    };

    debug!("decoded event: {event:?}");

    for deconstructed in validator.advance(&event)? {
        match deconstructed {
            StackFrame::Object | StackFrame::DirectoryEntry => expect(data, ")")?,
            StackFrame::RegularData { expected, .. } => padding(data, expected)?,
            _ => (),
        }
    }

    Ok(event)
}

#[inline]
fn split_to_checked(bytes: &mut Bytes, at: usize) -> Result<Bytes, Error> {
    let len = bytes.len();
    if at > len {
        Err(Error::Incomplete {
            needed: at - len,
        })
    } else {
        Ok(bytes.split_to(at))
    }
}

#[inline]
fn expect(data: &mut Bytes, expect: &str) -> Result<(), Error> {
    let found = string(data)?;

    trace!(
        "verifying that {expect:?} equals {:?}",
        String::from_utf8_lossy(&found)
    );

    if expect.as_bytes() != found {
        Err(Error::UnexpectedToken {
            expected: expect.to_string(),
            found,
        })
    } else {
        Ok(())
    }
}

#[inline]
fn padding(data: &mut Bytes, strlen: u64) -> Result<(), Error> {
    let padding = calculate_padding(strlen);
    trace!("discarding {padding} bytes of padding");

    let bytes = split_to_checked(data, padding)?;
    if bytes.iter().any(|b| *b != 0) {
        Err(Error::UnexpectedToken {
            expected: format!("{padding} bytes of padding"),
            found: bytes,
        })
    } else {
        Ok(())
    }
}

#[inline]
fn integer(data: &mut Bytes) -> Result<u64, Error> {
    let length = split_to_checked(data, size_of::<u64>())?;
    let length = u64::from_le_bytes(
        length
            .as_ref()
            .try_into()
            .expect("slice length should be size of u64"),
    );
    Ok(length)
}

#[inline]
fn string(data: &mut Bytes) -> Result<Bytes, Error> {
    let length = integer(data)?;
    trace!("extracting string of size {length}");

    let string = split_to_checked(data, length.try_into()?)?;
    padding(data, length)?;

    trace!("extracted string {:?}", String::from_utf8_lossy(&string));

    Ok(string)
}

fn with_peeked_string<R>(
    data: &mut Bytes,
    cmp: impl Fn(&[u8]) -> bool,
    success: impl Fn(&mut Bytes) -> R,
    failure: impl Fn(&mut Bytes) -> R,
) -> Result<R, Error> {
    debug!("peeking string");

    // clone is gross but wtv
    let mut attempt = data.clone();
    match string(&mut attempt) {
        Ok(str) => {
            if cmp(&str) {
                trace!("consuming string {:?}", String::from_utf8_lossy(&str));
                *data = attempt;
                Ok(success(data))
            } else {
                trace!("comparison failed");
                Ok(failure(data))
            }
        }
        Err(err) => Err(err),
    }
}

//! Decodes bytes into a NAR [`Event`] stream
//!
//! Keep in mind that [`Decoder`] allocates memory before
//! parsing events, which could lead to memory exhaustion
//! when parsing large events (such as [`Event::RegularContentChunk`])
//!
//! # Examples
//!
//! Decoding a NAR file from stdin into events on stderr
//!
//! ```rust,no_run
//! use nix_archive::decoding::Decoder;
//!
//! for event in Decoder::new().decode_reader(std::io::stdin()) {
//!     eprintln!("{:?}", event?);
//! }
//!
//! # Ok::<_, nix_archive::decoding::Error>(())
//! ```

use std::{fmt::Debug, io, num::TryFromIntError};

use bytes::{Bytes, BytesMut};
use thiserror::Error;

use crate::{
    Event,
    utils::{
        calculate_padding,
        log::{debug, trace},
    },
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
    /// Usually because underlying reader returned an error
    #[error(transparent)]
    IOError(#[from] io::Error),
    /// A number that was too big
    #[error(transparent)]
    ConversionError(#[from] TryFromIntError),
    /// The internal validator errored
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
    /// The provided data was not enough to form a complete [`Event`]
    #[error("input does not contain enough data")]
    Incomplete,
}

#[inline]
fn split_to_checked(bytes: &mut Bytes, at: usize) -> Result<Bytes, Error> {
    if at > bytes.len() {
        Err(Error::Incomplete)
    } else {
        Ok(bytes.split_to(at))
    }
}

#[inline]
fn expect(data: &mut Bytes, expect: &str) -> Result<Bytes, Error> {
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
        Ok(found)
    }
}

#[inline]
fn padding(data: &mut Bytes, strlen: u64) -> Result<(), Error> {
    let padding = calculate_padding(strlen);
    trace!("discarding {padding} bytes of padding");

    split_to_checked(data, padding)?;
    Ok(())
}

#[inline]
fn integer(data: &mut Bytes) -> Result<u64, Error> {
    let length = split_to_checked(data, size_of::<u64>())?;
    let length = u64::from_le_bytes(length.as_ref().try_into().unwrap());
    Ok(length)
}

fn string(data: &mut Bytes) -> Result<Bytes, Error> {
    let length = integer(data)?;
    trace!("extracting string of size {length}");

    let string = split_to_checked(data, length.try_into()?)?;
    padding(data, length)?;

    debug!("extracted string {:?}", String::from_utf8_lossy(&string));

    Ok(string)
}

fn with_peeked_string<R>(
    data: &mut Bytes,
    cmp: impl Fn(&[u8]) -> bool,
    success: impl Fn(&mut Bytes) -> R,
    failure: impl Fn(Option<&mut Bytes>) -> R,
) -> Result<R, Error> {
    debug!("peeking string");

    // clone is gross but wtv
    match string(&mut data.clone()) {
        Ok(str) => {
            if cmp(&str) {
                trace!("consuming string {:?}", String::from_utf8_lossy(&str));
                // consume
                string(data).unwrap();

                Ok(success(data))
            } else {
                trace!("comparison failed");
                Ok(failure(Some(data)))
            }
        }
        Err(Error::Incomplete) => Ok(failure(None)),
        Err(err) => Err(err),
    }
}

/// Decodes bytes into NAR [`Events`](Event)
#[derive(Debug)]
pub struct Decoder {
    state: EventValidator,
}

impl Decoder {
    /// Constructs a new [`Decoder`]
    #[inline]
    pub fn new() -> Self {
        Self {
            state: EventValidator::new(),
        }
    }

    /// Decode all events from a reader
    ///
    /// This method internally allocates an 8kb buffer, and
    /// will error with [`Error::IOError`], and an [`ErrorKind::StorageFull`](io::ErrorKind::StorageFull)
    pub fn decode_reader(
        &mut self,
        mut reader: impl io::Read,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        const SIZE: usize = 8192;

        let mut buffer = BytesMut::zeroed(SIZE);
        let mut position = 0;
        let mut errored = false;

        std::iter::from_fn(move || -> Option<Result<Event, Error>> {
            if self.state.finished() || errored {
                return None;
            }

            let result = loop {
                let frozen = std::mem::replace(&mut buffer, BytesMut::new()).freeze();
                let mut attempt = frozen.clone();
                debug!(
                    "{:?}",
                    attempt.iter().filter(|b| **b != 0).collect::<Vec<_>>()
                );

                match self.decode(&mut attempt) {
                    Err(err) => {
                        drop(attempt);
                        buffer = frozen.try_into_mut().unwrap();

                        match err {
                            Error::Incomplete => {
                                if position >= SIZE {
                                    break Err(io::Error::new(
                                        io::ErrorKind::StorageFull,
                                        "buffer exceeded the maximum capacity",
                                    )
                                    .into());
                                }

                                match reader.read(&mut buffer[position..]) {
                                    Err(err) => break Err(err.into()),
                                    Ok(0) => break Err(Error::Incomplete),
                                    Ok(n) => {
                                        debug!("progressing position by {n}");
                                        position += n;
                                    }
                                }
                            }
                            _ => break Err(err),
                        }
                    }
                    Ok(event) => {
                        let difference = frozen.len() - attempt.len();
                        position -= difference;
                        debug!("regressing position back by {difference}");

                        drop(frozen);
                        buffer = attempt.try_into_mut().unwrap();
                        break Ok(event);
                    }
                }
            };

            if let Err(_) = result {
                errored = true;
            }

            Some(result)
        })
    }

    /// Decode all events from an instance of [`Bytes`]
    ///
    /// The remaining [Bytes] instance will contain the data that the decoder did not consume
    pub fn decode_all(&mut self, data: &mut Bytes) -> impl Iterator<Item = Result<Event, Error>> {
        let mut errored = false;

        std::iter::from_fn(move || {
            if self.state.finished() || errored {
                return None;
            }

            if data.len() == 0 {
                errored = true;
                return Some(Err(Error::Incomplete));
            }

            let mut attempt = data.clone();
            let result = self.decode(&mut attempt);

            if let Ok(_) = result {
                *data = attempt;
            } else {
                errored = true;
            }

            Some(result)
        })
    }

    /// Decodes an individual event from the underlying reader
    ///
    /// This is a lower level method used usually used by [`decode_all`] or [`decode_reader`]
    /// Note that `data` will change even if [`decode`] errors
    pub fn decode(&mut self, data: &mut Bytes) -> Result<Event, Error> {
        debug!("UNIQUE STATE PRE: {}", data.is_unique());

        let frame = self.state.peek()?;
        debug!("decoding event with {frame:?} context frame ");

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
            StackFrame::DirectoryEntry => {
                unreachable!("directory entry stack frame should not be reachable")
            }
            StackFrame::RegularData { expected, written } => {
                // NOTE: ensure MAX_CHUNK_SIZE never goes above usize::MAX
                const MAX_CHUNK_SIZE: u64 = 4 * 1024;
                let size = (expected - written)
                    .min(data.len() as u64)
                    .min(MAX_CHUNK_SIZE) as usize;

                Event::RegularContentChunk(data.split_to(size))
            }
        };

        debug!("decoded event: {event:?}");

        for deconstructed in self.state.advance(&event)? {
            match deconstructed {
                StackFrame::Object | StackFrame::DirectoryEntry => {
                    expect(data, ")")?;
                }
                StackFrame::RegularData { expected, .. } => padding(data, expected)?,
                _ => (),
            }
        }

        debug!("UNIQUE STATE POST: {}", data.is_unique());
        Ok(event)
    }
}

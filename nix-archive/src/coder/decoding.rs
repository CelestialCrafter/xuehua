//! Decodes bytes into a NAR [`Event`] stream
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

use std::{collections::VecDeque, fmt::Debug, io, num::TryFromIntError};

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
    let length = u64::from_le_bytes(
        length
            .as_ref()
            .try_into()
            .expect("slice length should be size of u64"),
    );
    Ok(length)
}

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
    failure: impl Fn(Option<&mut Bytes>) -> R,
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

    /// Decode all events from a reader
    ///
    /// This method internally allocates a 4kb buffer for every chunk of events
    pub fn decode_reader(
        &mut self,
        mut reader: impl io::Read,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        // we can't use read_exact and ignore the error because the docs say:
        // > If this function encounters an “end of file” before completely filling
        // > the buffer, it returns an error of the kind ErrorKind::UnexpectedEof.
        // > The contents of buf are unspecified in this case.
        let mut fill_buffer = move |buffer: &mut BytesMut| {
            let initial = buffer.len();
            let mut position = initial;

            // inefficient but this library denies unsafe
            buffer.resize(buffer.len() + 4096, 0);

            while position < buffer.len() {
                match reader.read(&mut buffer[position..]) {
                    Ok(0) => break,
                    Ok(n) => position += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                    Err(e) => return Err(e),
                }
            }

            buffer.truncate(position);
            Ok(position - initial)
        };

        let mut byte_buffer = Bytes::new();
        let mut event_queue = VecDeque::new();
        let mut exhausted = false;

        std::iter::from_fn(move || -> Option<Result<Event, Error>> {
            let mut event = event_queue.pop_front();
            while let None | Some(Err(Error::Incomplete)) = event {
                if exhausted {
                    trace!("reader exhausted");
                    break;
                }

                // construct data starting from remaining bytes
                let mut data = BytesMut::from(byte_buffer.as_ref());
                trace!("event is {event:?}, attempting to refill starting with {data:?}");

                match fill_buffer(&mut data) {
                    Ok(0) => exhausted = true,
                    Ok(_) => {
                        byte_buffer = data.freeze();
                        event_queue.extend(self.decode_all(&mut byte_buffer));

                        event = event_queue.pop_front();
                    }
                    Err(err) => return Some(Err(err.into())),
                }
            }

            event
        })
    }

    /// Decode all events from an instance of [`Bytes`]
    ///
    /// The remaining [Bytes] instance will contain the remaining data that the decoder did not consume
    pub fn decode_all(&mut self, data: &mut Bytes) -> impl Iterator<Item = Result<Event, Error>> {
        let mut errored = false;
        std::iter::from_fn(move || {
            if data.len() == 0 || self.validator.finished() || errored {
                return None;
            }

            Some(self.decode(data).inspect_err(|_| errored = true))
        })
    }

    /// Decodes an individual event from the underlying reader
    ///
    /// This is a lower level method used usually used by [`decode_all`] or [`decode_reader`]
    /// Note that `data` will not change if [`decode`] errors
    pub fn decode(&mut self, data: &mut Bytes) -> Result<Event, Error> {
        let mut data_attempt = &mut data.clone();
        let mut state_attempt = self.validator.clone();
        let frame = state_attempt.peek()?;
        debug!("decoding event with {frame:?} context frame");

        let event = match frame {
            StackFrame::Header => {
                expect(data_attempt, "nix-archive-1")?;
                Event::Header
            }
            StackFrame::Object => {
                expect(data_attempt, "(")?;
                expect(data_attempt, "type")?;
                let ty = string(data_attempt)?;
                match ty.as_ref() {
                    b"regular" => {
                        let executable = with_peeked_string(
                            data_attempt,
                            |str| str == b"executable",
                            |data| {
                                expect(data, "")?;
                                Ok::<_, Error>(true)
                            },
                            |_| Ok(false),
                        )??;

                        expect(data_attempt, "contents")?;

                        Event::Regular {
                            executable,
                            size: integer(data_attempt)?,
                        }
                    }
                    b"symlink" => {
                        expect(data_attempt, "target")?;
                        Event::Symlink {
                            target: string(data_attempt)?,
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
                data_attempt,
                |str| str == b"entry",
                |data| {
                    expect(data, "(")?;
                    expect(data, "name")?;
                    let name = string(data)?;
                    expect(data, "node")?;

                    Ok::<_, Error>(Event::DirectoryEntry { name })
                },
                |data| match data {
                    Some(_) => Ok(Event::DirectoryEnd),
                    None => Err(Error::Incomplete),
                },
            )??,
            StackFrame::DirectoryEntry => panic!("peeked frame was directory entry"),
            StackFrame::RegularData { expected, written } => {
                // NOTE: ensure MAX_CHUNK_SIZE never goes above usize::MAX
                const MAX_CHUNK_SIZE: u64 = 4 * 1024;
                let size = (expected - written)
                    .min(data_attempt.len() as u64)
                    .min(MAX_CHUNK_SIZE) as usize;

                Event::RegularContentChunk(data_attempt.split_to(size))
            }
        };

        debug!("decoded event: {event:?}");

        for deconstructed in state_attempt.advance(&event)? {
            match deconstructed {
                StackFrame::Object | StackFrame::DirectoryEntry => {
                    expect(&mut data_attempt, ")")?;
                }
                StackFrame::RegularData { expected, .. } => padding(&mut data_attempt, expected)?,
                _ => (),
            }
        }

        *data = std::mem::replace(data_attempt, Bytes::new());
        self.validator = state_attempt;
        Ok(event)
    }
}

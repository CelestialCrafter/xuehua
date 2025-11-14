use std::{
    ffi::OsString, fmt::Debug, io, num::TryFromIntError, os::unix::ffi::OsStringExt, str::Utf8Error,
};

use log::{debug, trace};
use thiserror::Error;

use crate::{
    state::{CoderState, Error as CoderStateError, Event, StackFrame},
    utils::{PADDING, calculate_padding},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected token: expected {expected}, found {found}")]
    UnexpectedToken { expected: String, found: String },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    Utf8Error(#[from] Utf8Error),
    #[error(transparent)]
    ConversionError(#[from] TryFromIntError),
    #[error(transparent)]
    CoderError(#[from] CoderStateError),
}

fn unexpected(expected: &str, found: &[u8]) -> Error {
    Error::UnexpectedToken {
        expected: format!("{:?}", expected),
        found: format!("{:?}", String::from_utf8_lossy(found)),
    }
}

#[derive(Debug)]
pub struct Decoder<R> {
    state: CoderState,
    lookahead: Option<Vec<u8>>,
    reader: R,
}

impl<R: io::Read> Iterator for Decoder<R> {
    type Item = Result<Event, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (!self.state.finished()).then(|| self.decode())
    }
}

impl<R: io::Read> Decoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            state: CoderState::new(),
            lookahead: None,
            reader,
        }
    }

    fn decode(&mut self) -> Result<Event, Error> {
        let bytes_to_path = |bytes| OsString::from_vec(bytes).into();

        let frame = self.state.peek()?;
        debug!("decoding event with {frame:?} context frame ");

        let event = match frame {
            StackFrame::Header => {
                self.expect("nix-archive-1")?;
                Event::Header
            }
            StackFrame::Object => {
                self.expect("(")?;
                self.expect("type")?;
                match self.string()?.as_slice() {
                    b"regular" => {
                        let executable = match self.peek_string()? {
                            Some(b"executable") => {
                                self.lookahead = None;
                                self.expect("")?;
                                true
                            }
                            _ => false,
                        };

                        self.expect("contents")?;

                        Event::Regular {
                            executable,
                            size: self.integer()?,
                        }
                    }
                    b"symlink" => {
                        self.expect("target")?;
                        Event::Symlink {
                            target: bytes_to_path(self.string()?),
                        }
                    }
                    b"directory" => Event::Directory,
                    ty => return Err(unexpected(r#""regular", "symlink", or "directory""#, ty)),
                }
            }
            StackFrame::Directory => match self.peek_string()? {
                Some(b"entry") => {
                    self.lookahead = None;
                    self.expect("(")?;
                    self.expect("name")?;
                    let name = bytes_to_path(self.string()?);
                    self.expect("node")?;

                    Event::DirectoryEntry { name }
                }
                _ => Event::DirectoryEnd,
            },
            StackFrame::DirectoryEntry => {
                unreachable!("directory entry stack frame should not be reachable")
            }
            StackFrame::RegularData { expected, written } => {
                const MAX_CHUNK_SIZE: u64 = 4 * 1024;

                let mut buffer = vec![0; (expected - written).min(MAX_CHUNK_SIZE) as usize];
                self.reader.read_exact(&mut buffer)?;

                Event::RegularContentChunk(buffer)
            }
        };

        debug!("decoded event: {event:?}");

        for deconstructed in self.state.advance(&event)? {
            match deconstructed {
                StackFrame::Object | StackFrame::DirectoryEntry => self.expect(")")?,
                StackFrame::RegularData { expected, .. } => self.padding(expected)?,
                _ => (),
            }
        }

        Ok(event)
    }

    fn expect(&mut self, expect: &str) -> Result<(), Error> {
        let found = self.string()?;
        trace!(
            "verifying that {expect:?} equals {:?}",
            String::from_utf8_lossy(&found)
        );
        if expect.as_bytes() != found {
            Err(unexpected(expect, &found))
        } else {
            Ok(())
        }
    }

    fn padding(&mut self, strlen: u64) -> Result<(), Error> {
        let padding = calculate_padding(strlen);
        trace!("discarding {padding} bytes of padding");

        let mut buffer = [0; PADDING];
        self.reader.read_exact(&mut buffer[..padding])?;

        if buffer.into_iter().any(|v| v != 0) {
            Err(unexpected(&format!("{padding} bytes of padding"), &buffer))
        } else {
            Ok(())
        }
    }

    fn integer(&mut self) -> Result<u64, Error> {
        let mut len = [0; size_of::<u64>()];
        self.reader.read_exact(&mut len)?;
        Ok(u64::from_le_bytes(len))
    }

    fn peek_string(&mut self) -> Result<Option<&[u8]>, Error> {
        if self.lookahead.is_none() {
            match self.consume_string() {
                Ok(v) => self.lookahead = Some(v),
                Err(Error::IOError(err)) if err.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(None);
                }
                Err(err) => return Err(err),
            };
        }

        Ok(Some(self.lookahead.as_ref().unwrap()))
    }

    fn string(&mut self) -> Result<Vec<u8>, Error> {
        match self.lookahead.take() {
            Some(v) => Ok(v),
            None => self.consume_string(),
        }
    }

    fn consume_string(&mut self) -> Result<Vec<u8>, Error> {
        let length = self.integer()?;
        trace!("extracting string of size {length}");

        let mut data = vec![0; length.try_into()?];
        self.reader.read_exact(&mut data)?;
        self.padding(length)?;

        debug!("extracted string {:?}", String::from_utf8_lossy(&data));
        Ok(data)
    }
}

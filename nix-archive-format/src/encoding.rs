//! Encoder for NAR data streams
//!
//! One thing to keep in mind is that [`Encoder`] has an internal buffer,
//! Which means it will allocate as much memory as needed to contain an event.
//! This is usually not an issue because:
//! - All events except for [`Event::RegularContentChunk`]'s are tiny
//! - The decoder chunks [`Event::RegularContentChunk`] events to a reasonable size
//!
//! But if you encode custom event streams, keep this in mind.
//!
//! # Examples
//!
//! Encoding NAR events to a NAR file on stdout
//!
//! ```rust
//! use nix_archive_format::{encoding::Encoder, state::Event};
//!
//! let content = "hello world!";
//! let events = vec![
//!     Event::Header,
//!     Event::Directory,
//!     Event::DirectoryEntry {
//!         name: std::path::PathBuf::from("my-file"),
//!     },
//!     Event::Regular {
//!         executable: true,
//!         size: content.len() as u64,
//!     },
//!     Event::RegularContentChunk(content.as_bytes().to_vec()),
//!     Event::DirectoryEnd,
//! ];
//!
//! std::io::copy(&mut Encoder::new(events.iter()), &mut std::io::stdout())?;
//!
//! # Ok::<_, std::io::Error>(())
//! ```

use std::{
    io::{self, Write},
    iter::repeat,
    path::Path,
    str::Utf8Error,
};

use thiserror::Error;

use crate::{
    state::{CoderState, Error as CoderStateError, Event, StackFrame},
    utils::{
        calculate_padding,
        log::{debug, trace},
    },
};

/// Error type for the [Encoder]
#[derive(Error, Debug)]
pub enum Error {
    /// The internal state errored, usually because the underlying reader had its events in an incorrect order
    #[error(transparent)]
    CoderError(#[from] CoderStateError),
    /// Usually due to paths being non-UTF-8
    #[error(transparent)]
    Utf8Error(#[from] Utf8Error),
}

/// Encodes NAR [Events](Event) into bytes
#[derive(Debug)]
pub struct Encoder<I> {
    state: CoderState,
    buffer: Vec<u8>,
    position: usize,
    events: I,
}

impl<'a, I: Iterator<Item = &'a Event>> io::Read for Encoder<I> {
    fn read(&mut self, mut buffer: &mut [u8]) -> Result<usize, io::Error> {
        if self.position == self.buffer.len() && !self.state.finished() {
            self.buffer.clear();
            self.position = 0;

            let event = self
                .events
                .next()
                .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))?;

            self.encode(event)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        }

        let n = buffer.write(&self.buffer[self.position..])?;
        self.position += n;
        Ok(n)
    }
}

fn path_to_str(path: &Path) -> Result<&str, Utf8Error> {
    str::from_utf8(path.as_os_str().as_encoded_bytes())
}

impl<'a, I: Iterator<Item = &'a Event>> Encoder<I> {
    /// Constructs a new [`Encoder`] from an [`Iterator`] of [`Events`](Event)
    pub fn new(events: I) -> Self {
        Self {
            events,
            position: 0,
            buffer: Default::default(),
            state: CoderState::new(),
        }
    }

    fn encode(&mut self, event: &Event) -> Result<(), Error> {
        debug!("encoding event: {event:?}");

        match event {
            Event::Header => self.string("nix-archive-1"),
            Event::Regular { executable, size } => {
                self.string("(");
                self.string("type");
                self.string("regular");

                if *executable {
                    self.string("executable");
                    self.string("");
                }

                self.string("contents");
                self.integer(*size);
            }
            Event::RegularContentChunk(chunk) => self.buffer.extend(chunk),
            Event::Symlink { target } => {
                self.string("(");
                self.string("type");
                self.string("symlink");
                self.string("target");
                self.string(path_to_str(target)?);
            }
            Event::Directory => {
                self.string("(");
                self.string("type");
                self.string("directory");
            }
            Event::DirectoryEntry { name } => {
                self.string("entry");
                self.string("(");
                self.string("name");
                self.string(path_to_str(name)?);
                self.string("node");
            }
            Event::DirectoryEnd => (),
        }

        for deconstructed in self.state.advance(&event)? {
            match deconstructed {
                StackFrame::Object | StackFrame::DirectoryEntry => self.string(")"),
                StackFrame::RegularData { expected, .. } => self.padding(expected),
                _ => (),
            }
        }

        Ok(())
    }

    fn padding(&mut self, strlen: u64) {
        let padding = calculate_padding(strlen);
        trace!("writing {padding} bytes of padding");
        self.buffer.extend(repeat(0).take(padding));
    }

    fn integer(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: impl AsRef<[u8]>) {
        let data = value.as_ref();
        let len = data.len() as u64;

        trace!(
            "writing string {:?} of size {len}",
            String::from_utf8_lossy(data)
        );

        self.integer(len);
        self.buffer.extend_from_slice(data);
        self.padding(len)
    }
}

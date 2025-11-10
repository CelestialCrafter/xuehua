//! Encoder for NAR data streams
//!
//! # Examples
//!
//! Encoding NAR events to a NAR file on stdout
//!
//! ```rust
//! use nix_archive::{encoding::Encoder, state::Event};
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
//! Encoder::new(std::io::stdout()).copy(events.iter())?;
//!
//! # Ok::<_, nix_archive::encoding::Error>(())
//! ```

use std::{io, str::Utf8Error};

use thiserror::Error;

use crate::{
    state::{CoderState, Error as CoderStateError, Event, StackFrame},
    utils::{
        PADDING, calculate_padding,
        log::{debug, trace},
    },
};

/// Error type for the [Encoder]
#[derive(Error, Debug)]
pub enum Error {
    /// Usually due to an error in the underlying writer
    #[error(transparent)]
    IOError(#[from] io::Error),
    /// The internal state errored, usually because the underlying reader had its events in an incorrect order
    #[error(transparent)]
    CoderError(#[from] CoderStateError),
    /// Usually due to paths being non-UTF-8
    #[error(transparent)]
    Utf8Error(#[from] Utf8Error),
}

/// Encodes NAR [Events](Event) into bytes
#[derive(Debug)]
pub struct Encoder<W> {
    state: CoderState,
    writer: W,
}

impl<'a, W: io::Write> Encoder<W> {
    /// Constructs a new [`Encoder`] from an [`Iterator`] of [`Events`](Event)
    #[inline]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            state: CoderState::new(),
        }
    }

    /// "Copies" the events from an iterator into the encoder
    pub fn copy<I: Iterator<Item = &'a Event>>(&mut self, mut iterator: I) -> Result<(), Error> {
        iterator.try_for_each(|event| self.encode(event))
    }

    /// Encodes a single event into the writer
    pub fn encode(&mut self, event: &Event) -> Result<(), Error> {
        debug!("encoding event: {event:?}");

        match event {
            Event::Header => self.string("nix-archive-1")?,
            Event::Regular { executable, size } => {
                self.string("(")?;
                self.string("type")?;
                self.string("regular")?;

                if *executable {
                    self.string("executable")?;
                    self.string("")?;
                }

                self.string("contents")?;
                self.integer(*size)?;
            }
            Event::RegularContentChunk(chunk) => self.writer.write_all(&chunk)?,
            Event::Symlink { target } => {
                self.string("(")?;
                self.string("type")?;
                self.string("symlink")?;
                self.string("target")?;
                self.string(str::from_utf8(target.as_os_str().as_encoded_bytes())?)?;
            }
            Event::Directory => {
                self.string("(")?;
                self.string("type")?;
                self.string("directory")?;
            }
            Event::DirectoryEntry { name } => {
                self.string("entry")?;
                self.string("(")?;
                self.string("name")?;
                self.string(str::from_utf8(name.as_encoded_bytes())?)?;
                self.string("node")?;
            }
            Event::DirectoryEnd => (),
        }

        for deconstructed in self.state.advance(&event)? {
            match deconstructed {
                StackFrame::Object | StackFrame::DirectoryEntry => self.string(")")?,
                StackFrame::RegularData { expected, .. } => self.padding(expected)?,
                _ => (),
            }
        }

        Ok(())
    }

    #[inline]
    fn padding(&mut self, strlen: u64) -> Result<(), io::Error> {
        let padding = calculate_padding(strlen);
        trace!("writing {padding} bytes of padding");

        let buffer = [0; PADDING];
        self.writer.write_all(&buffer[..padding])
    }

    #[inline]
    fn integer(&mut self, value: u64) -> Result<(), io::Error> {
        self.writer.write_all(&value.to_le_bytes())
    }

    #[inline]
    fn string(&mut self, value: impl AsRef<[u8]>) -> Result<(), io::Error> {
        let data = value.as_ref();
        let len = data.len() as u64;

        trace!(
            "writing string {:?} of size {len}",
            String::from_utf8_lossy(data)
        );

        self.integer(len)?;
        self.writer.write_all(data)?;
        self.padding(len)
    }
}

use std::{
    io::{self, Write},
    iter::repeat,
};

use log::{debug, trace};
use thiserror::Error;

use crate::{
    state::{CoderState, Error as CoderStateError, Event, StackFrame},
    utils::calculate_padding,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    CoderError(#[from] CoderStateError),
}

#[derive(Debug)]
pub struct Encoder<I> {
    state: CoderState,
    buffer: Vec<u8>,
    events: I,
}

impl<'a, I: Iterator<Item = &'a Event>> io::Read for Encoder<I> {
    fn read(&mut self, mut buffer: &mut [u8]) -> Result<usize, io::Error> {
        if self.state.finished() {
            return Ok(0);
        }

        let event = self
            .events
            .next()
            .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))?;

        self.encode(event)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        let n = self.buffer.len();
        buffer.write_all(&self.buffer)?;
        Ok(n)
    }
}

impl<'a, I: Iterator<Item = &'a Event>> Encoder<I> {
    pub fn new(events: I) -> Self {
        Self {
            events,
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
                self.string(target.as_os_str().as_encoded_bytes());
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
                self.string(name.as_os_str().as_encoded_bytes());
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

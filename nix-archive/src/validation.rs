//! Internal event stream validation
//!
//! This module is generally only important to the crate's internals.

use alloc::{vec, vec::Vec};
use core::cmp::Ordering;

use bytes::Bytes;
use thiserror::Error;

use crate::{Event, utils::debug};

/// The error type for the internal event validator
#[derive(Error, Debug)]
pub enum Error {
    /// The validator has finished, and should've not have received more events
    #[error("validation has finished")]
    Finished,
    /// A directory entry was given in non-alphabetical order
    #[error("directory entry {0:?} is not in alphabetical order")]
    UnsortedEntry(Bytes),
    /// The processed event was invalid for the current parse state
    #[error("unexpected event {0:?} in state {1:?}")]
    UnexpectedEvent(Event, StackFrame),
}

/// A frame of the validator's internal stack
///
/// This struct is internal and not be used.It's only public so it can be used in [`enum@Error`].
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq)]
pub enum StackFrame {
    Header,
    Object,
    Directory { last: Option<Bytes> },
    DirectoryEntry,
    RegularData { expected: u64, written: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct EventValidator {
    stack: Vec<StackFrame>,
}

impl EventValidator {
    #[inline]
    pub fn new() -> Self {
        Self {
            stack: vec![StackFrame::Header],
        }
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.stack.is_empty()
    }

    #[inline]
    pub fn peek(&self) -> Result<&StackFrame, Error> {
        self.stack.last().ok_or(Error::Finished)
    }

    #[inline]
    fn deconstruct(&mut self, actions: &mut Vec<StackFrame>) {
        let frame = self.stack.pop().expect("stack should not be empty");
        debug!("deconstructing frame: {frame:?}");
        actions.push(frame);
    }

    #[inline]
    fn construct(&mut self, frame: StackFrame) {
        debug!("constructing frame: {frame:?}");
        self.stack.push(frame);
    }

    #[inline]
    fn post_object(&mut self, actions: &mut Vec<StackFrame>) {
        self.deconstruct(actions);
        if let Some(StackFrame::DirectoryEntry) = self.stack.last() {
            self.deconstruct(actions);
        };
    }

    pub fn advance(&mut self, event: &Event) -> Result<Vec<StackFrame>, Error> {
        debug!("advancing validator from {:?}", self.stack);

        let mut deconstructed = vec![];
        let frame = self.stack.last().ok_or(Error::Finished)?.clone();

        match (frame, event) {
            (StackFrame::Header, Event::Header) => {
                self.deconstruct(&mut deconstructed);
                self.construct(StackFrame::Object);
            }
            (
                StackFrame::Object,
                Event::Regular { .. } | Event::Symlink { .. } | Event::Directory,
            ) => match event {
                Event::Regular { size, .. } => {
                    let regular = StackFrame::RegularData {
                        expected: *size,
                        written: 0,
                    };

                    self.construct(regular);
                }
                Event::Symlink { .. } => self.post_object(&mut deconstructed),
                Event::Directory => self.construct(StackFrame::Directory { last: None }),
                _ => unreachable!(),
            },
            (StackFrame::Directory { last }, Event::DirectoryEntry { name }) => {
                if let Some(last) = last {
                    if last >= name {
                        return Err(Error::UnsortedEntry(name.clone()));
                    }
                }

                match self.stack.last_mut().unwrap() {
                    StackFrame::Directory { last } => *last = Some(name.clone()),
                    _ => unreachable!(),
                }

                self.construct(StackFrame::DirectoryEntry);
                self.construct(StackFrame::Object);
            }
            (StackFrame::Directory { .. }, Event::DirectoryEnd) => {
                self.deconstruct(&mut deconstructed);
                self.post_object(&mut deconstructed);
            }
            (StackFrame::RegularData { expected, .. }, Event::RegularContentChunk(chunk)) => {
                let frame = self.stack.last_mut().unwrap();
                let written = match frame {
                    StackFrame::RegularData { written, .. } => {
                        *written += chunk.len() as u64;
                        written
                    }
                    _ => unreachable!(),
                };

                match (*written).cmp(&expected) {
                    Ordering::Less => (),
                    Ordering::Equal => {
                        self.deconstruct(&mut deconstructed);
                        self.post_object(&mut deconstructed);
                    }
                    Ordering::Greater => return Err(Error::UnexpectedEvent(event.clone(), frame.clone())),
                }
            }
            (frame, _) => return Err(Error::UnexpectedEvent(event.clone(), frame.clone())),
        }

        Ok(deconstructed)
    }
}

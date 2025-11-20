//! Internal event stream validation
//!
//! This module is generally only important to the crate's internals,
//! but if you do need stuff from this module,
//! you probably want [`Event`] or [`enum@Error`]

#[allow(dead_code)]
pub(crate) mod arbitrary;

use thiserror::Error;

use crate::{Event, utils::log::debug};

/// The error type for the internal event validator
#[derive(Error, Debug)]
pub enum Error {
    /// The validator has finished, and should've not have received more events
    #[error("validation has finished")]
    Finished,
    /// The processed event was invalid for the current parse state
    #[error("unexpected event {0:?} in state {1:?}")]
    Unexpected(Event, StackFrame),
}

/// A frame of the validator's internal stack
///
/// This struct is internal and not be used.It's only public so it can be used in [`enum@Error`].
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StackFrame {
    Header,
    Object,
    Directory,
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
    pub fn peek(&self) -> Result<StackFrame, Error> {
        self.stack.last().ok_or(Error::Finished).copied()
    }

    #[inline]
    fn deconstruct(&mut self, actions: &mut Vec<StackFrame>, expected: StackFrame) {
        let frame = self.stack.pop().expect("stack should not be empty");
        debug!("deconstructing frame: {frame:?}");
        assert_eq!(frame, expected, "popped frame did not equal expected frame");
        actions.push(frame);
    }

    #[inline]
    fn construct(&mut self, frame: StackFrame) {
        debug!("constructing frame: {frame:?}");
        self.stack.push(frame);
    }

    #[inline]
    fn post_object(&mut self, actions: &mut Vec<StackFrame>) {
        self.deconstruct(actions, StackFrame::Object);
        if let Some(StackFrame::DirectoryEntry) = self.stack.last() {
            self.deconstruct(actions, StackFrame::DirectoryEntry);
        };
    }

    pub fn advance(&mut self, event: &Event) -> Result<Vec<StackFrame>, Error> {
        debug!("advancing validator from {:?}", self.stack);

        let mut deconstructed = vec![];
        let frame = *self.stack.last().ok_or(Error::Finished)?;
        let unexpected = || Err(Error::Unexpected(event.clone(), frame));

        match (frame, event) {
            (StackFrame::Header, Event::Header) => {
                self.deconstruct(&mut deconstructed, frame);
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
                Event::Directory => self.construct(StackFrame::Directory),
                _ => unreachable!(),
            },
            (StackFrame::Directory, Event::DirectoryEntry { .. }) => {
                self.construct(StackFrame::DirectoryEntry);
                self.construct(StackFrame::Object);
            }
            (StackFrame::Directory, Event::DirectoryEnd) => {
                self.deconstruct(&mut deconstructed, frame);
                self.post_object(&mut deconstructed);
            }
            (StackFrame::RegularData { expected, written }, Event::RegularContentChunk(chunk)) => {
                let written = written + chunk.len() as u64;

                let frame = self.stack.pop().expect("stack frame should exist");
                if expected == written {
                    debug!("deconstructing frame: {frame:?}");
                    deconstructed.push(frame);
                    self.post_object(&mut deconstructed);
                } else if written > expected {
                    return unexpected();
                } else {
                    self.construct(StackFrame::RegularData {
                        expected,
                        written: written,
                    });
                }
            }
            _ => return unexpected(),
        }

        Ok(deconstructed)
    }
}

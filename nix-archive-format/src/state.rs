pub mod arbitrary;

use std::path::PathBuf;

use thiserror::Error;

use crate::utils::log::debug;

#[derive(Error, Debug)]
pub enum Error {
    #[error("coding has finished")]
    Finished,
    #[error("unexpected event {1:?} in state {0:?}")]
    Unexpected(StackFrame, Event),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Header,
    Regular { executable: bool, size: u64 },
    RegularContentChunk(Vec<u8>),
    Symlink { target: PathBuf },
    Directory,
    DirectoryEntry { name: PathBuf },
    DirectoryEnd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StackFrame {
    Header,
    Object,
    Directory,
    DirectoryEntry,
    RegularData { expected: u64, written: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct CoderState {
    stack: Vec<StackFrame>,
}

impl CoderState {
    pub fn new() -> Self {
        Self {
            stack: vec![StackFrame::Header],
        }
    }

    pub fn finished(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn peek(&self) -> Result<StackFrame, Error> {
        self.stack.last().ok_or(Error::Finished).copied()
    }

    fn deconstruct(&mut self, actions: &mut Vec<StackFrame>, expected: StackFrame) {
        let frame = self.stack.pop().expect("stack should not be empty");
        debug!("deconstructing frame: {frame:?}");
        assert_eq!(frame, expected, "popped frame did not equal expected frame");
        actions.push(frame);
    }

    fn construct(&mut self, frame: StackFrame) {
        debug!("constructing frame: {frame:?}");
        self.stack.push(frame);
    }

    fn post_object(&mut self, actions: &mut Vec<StackFrame>) {
        self.deconstruct(actions, StackFrame::Object);
        if let Some(StackFrame::DirectoryEntry) = self.stack.last() {
            self.deconstruct(actions, StackFrame::DirectoryEntry);
        };
    }

    pub fn advance(&mut self, event: &Event) -> Result<Vec<StackFrame>, Error> {
        debug!("advancing coder state from {:?}", self.stack);

        let mut deconstructed = vec![];
        let frame = *self.stack.last().ok_or(Error::Finished)?;
        let unexpected = || Err(Error::Unexpected(frame, event.clone()));

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
                    // ensure at least 1 regular chunk is emitted to properly handle ending regular
                    deconstructed.extend(self.advance(&Event::RegularContentChunk(vec![]))?);
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

                let frame = self.stack.pop().unwrap();
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

//! Encoding of [`Event`]s into binary

use alloc::borrow::Cow;
use core::borrow::Borrow;

use bytes::{BufMut, BytesMut};
use thiserror::Error;

use crate::{
    Event, Object,
    hashing::Hasher,
    utils::{State, debug},
};

/// Error type for encoding
#[derive(Error, Debug)]
pub enum Error {
    /// An invalid event was provided
    #[error("unexpected event: {event:?} (expected: {expected})")]
    UnexpectedEvent {
        #[allow(missing_docs)]
        event: Event,
        #[allow(missing_docs)]
        expected: Cow<'static, str>,
    },
    #[allow(missing_docs)]
    #[cfg(feature = "std")]
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

/// Encoder for archive events
///
/// The encoder consumes [`Event`]s and outputs binary.
///
/// An encoder can only encode a single archive.
/// Once [`finished`] returns true, no further events can be encoded.
#[derive(Default)]
pub struct Encoder {
    state: State,
}

impl Encoder {
    /// Constructs a new encoder.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns whether or not the encoder has completed.
    #[inline]
    pub fn finished(&self) -> bool {
        self.state.finished()
    }

    /// Encodes an iterator of [`Event`]s into a `buffer`.
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `buffer` are unmodified,
    /// and this function may be retried.
    #[inline]
    pub fn encode(
        &mut self,
        buffer: &mut BytesMut,
        events: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        let mut start = 0;
        events
            .into_iter()
            .try_for_each(|event| {
                start = buffer.len();
                self.process(buffer, event.borrow())
            })
            .inspect_err(|_| buffer.truncate(start))
    }

    /// Encodes an iterator of [`Event`]s into a `writer`
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `writer` are unmodified,
    /// and this function may be retried.
    #[cfg(feature = "std")]
    #[inline]
    pub fn encode_writer(
        &mut self,
        writer: &mut impl std::io::Write,
        events: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        let mut buffer = BytesMut::with_capacity(4096);
        events.into_iter().try_for_each(|event| {
            self.process(&mut buffer, event.borrow())?;
            writer.write_all(&buffer)?;
            buffer.clear();

            Ok(())
        })
    }

    fn process(&mut self, buffer: &mut impl BufMut, event: &Event) -> Result<(), Error> {
        debug!("encoding event {event:?} in state {:?}", self.state);

        match self.state {
            State::Magic => {
                debug!("encoding magic");

                buffer.put_slice(b"xuehua-archive");
                buffer.put_u16_le(1);

                self.state = State::Index;
                return self.process(buffer, event);
            }
            State::Index => {
                let Event::Index(index) = event else {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        expected: "index event".into(),
                    });
                };

                let amount = index.len() as u64;
                buffer.put_u64_le(amount);
                index.iter().for_each(|(path, metadata)| {
                    buffer.put_u64_le(path.inner.len() as u64);
                    buffer.put_slice(&path.inner);

                    buffer.put_u32_le(metadata.permissions);
                    buffer.put_u64_le(metadata.size);
                    buffer.put_u8(metadata.variant as u8);
                });

                self.state = State::Objects(amount);
            }
            State::Objects(amount) => {
                if amount == 0 {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        expected: "no more events".into(),
                    });
                }

                let Event::Object(object) = event else {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        expected: "object event".into(),
                    });
                };

                match object {
                    Object::File { contents } => buffer.put_slice(&contents),
                    Object::Symlink { target } => buffer.put_slice(&target.inner),
                    Object::Directory => (),
                };

                match self.state {
                    State::Objects(ref mut amount) => *amount -= 1,
                    _ => unreachable!("should not be called if amount == 0"),
                };
            }
        }

        buffer.put_slice(Hasher::hash(event).as_bytes());
        Ok(())
    }
}

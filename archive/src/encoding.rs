use alloc::borrow::Cow;
use core::borrow::Borrow;

use bytes::BufMut;
use thiserror::Error;

use crate::{
    Event, Object,
    hashing::Hasher,
    utils::{State, debug},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: \"{event:?}\" (expected: {expected})")]
    Unexpected {
        event: Event,
        expected: Cow<'static, str>,
    },
}

pub struct Encoder<'a, B> {
    state: State,
    buffer: &'a mut B,
}

impl<'a, B: BufMut> Encoder<'a, B> {
    #[inline]
    pub fn new(buffer: &'a mut B) -> Self {
        Self {
            state: Default::default(),
            buffer,
        }
    }

    #[inline]
    pub fn with_buffer<'b, T>(self, buffer: &'b mut T) -> Encoder<'b, T>
    where
        T: BufMut,
    {
        Encoder {
            state: self.state,
            buffer: buffer,
        }
    }

    #[inline]
    pub fn encode(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow()))
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.state.finished()
    }

    fn process(&mut self, event: &Event) -> Result<(), Error> {
        debug!("encoding event {event:?} in state {:?}", self.state);

        match self.state {
            State::Magic => {
                debug!("encoding magic");

                self.buffer.put_slice(b"xuehua-archive");
                self.buffer.put_u16_le(1);

                self.state = State::Index;
                return self.process(event);
            }
            State::Index => {
                let Event::Index(index) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        expected: "index event".into(),
                    });
                };

                let amount = index.len() as u64;
                self.buffer.put_u64_le(amount);
                index.iter().for_each(|(path, metadata)| {
                    self.buffer.put_u64_le(path.inner.len() as u64);
                    self.buffer.put_slice(&path.inner);

                    self.buffer.put_u32_le(metadata.permissions);
                    self.buffer.put_u64_le(metadata.size);
                    self.buffer.put_u8(metadata.variant as u8);
                });

                self.state = State::Objects(amount);
            }
            State::Objects(amount) => {
                if amount == 0 {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        expected: "no more events".into(),
                    });
                }

                let Event::Object(object) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        expected: "object event".into(),
                    });
                };

                match object {
                    Object::File { contents } => self.buffer.put_slice(&contents),
                    Object::Symlink { target } => self.buffer.put_slice(&target.inner),
                    Object::Directory => (),
                };

                match self.state {
                    State::Objects(ref mut amount) => *amount -= 1,
                    _ => unreachable!("should not be called if amount == 0"),
                };
            }
        }

        self.buffer.put_slice(Hasher::hash(event).as_bytes());
        Ok(())
    }
}

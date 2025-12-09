use alloc::borrow::Cow;
use core::borrow::Borrow;

use blake3::Hasher;
use bytes::BufMut;
use thiserror::Error;

use crate::{
    Contents, Event, Object, Operation,
    utils::{State, debug, hash_plen},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: \"{event:?}\" ({reason})")]
    Unexpected {
        event: Event,
        reason: Cow<'static, str>,
    },
    #[error("not enough events were processed")]
    Incomplete,
    #[error("file contents should be compressed")]
    Decompressed,
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
    pub fn finish(self) -> Result<(), Error> {
        debug!("finishing encoder");

        match self.state {
            State::Magic => Err(Error::Incomplete),
            State::Index => Err(Error::Incomplete),
            State::Operations(amount) => {
                if amount > 1 {
                    Err(Error::Incomplete)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn process(&mut self, event: &Event) -> Result<(), Error> {
        debug!("encoding event {event:?} in state {:?}", self.state);

        match self.state {
            State::Magic => {
                debug!("encoding magic");

                self.buffer.put_slice(b"xuehua-archive");
                self.buffer.put_u16_le(1);

                self.state = State::Index;
                self.process(event)
            }
            State::Index => {
                let Event::Index(index) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "need index event".into(),
                    });
                };

                let mut hasher = Hasher::new();
                self.buffer.put_u64_le(index.len() as u64);
                index.iter().for_each(|path| {
                    self.put_plen(&path.inner);
                    hash_plen(&mut hasher, &path.inner);
                });
                let hash = hasher.finalize();
                debug!("index hashed to {hash}");
                self.buffer.put_slice(hash.as_bytes());

                self.state = State::Operations(index.len());
                Ok(())
            }
            State::Operations(amount) => {
                let Event::Operation(operation) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "need operation event".into(),
                    });
                };

                if amount == 0 {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "excess event".into(),
                    });
                }

                match operation {
                    Operation::Create {
                        permissions,
                        object,
                        ..
                    } => self.put_create_op(*permissions, object)?,
                    Operation::Delete { .. } => self.buffer.put_u8(1),
                };

                let mut hasher = Hasher::new();
                operation.hash(&mut hasher);
                let hash = hasher.finalize();
                debug!("operation hashed to {hash}");
                self.buffer.put_slice(hash.as_bytes());

                match self.state {
                    State::Operations(ref mut amount) => *amount -= 1,
                    _ => unreachable!(),
                };

                Ok(())
            }
        }
    }

    fn put_create_op(&mut self, permissions: u32, object: &Object) -> Result<(), Error> {
        self.buffer.put_u8(0);
        self.buffer.put_u32_le(permissions);

        match object {
            Object::File { contents, prefix } => {
                self.buffer.put_u8(0);
                match prefix {
                    None => self.buffer.put_u8(0),
                    Some(hash) => {
                        self.buffer.put_u8(1);
                        self.buffer.put_slice(hash.as_bytes());
                    }
                }

                match contents {
                    Contents::Compressed(bytes) => self.put_plen(bytes),
                    Contents::Decompressed(_) => return Err(Error::Decompressed),
                }
            }
            Object::Symlink { target } => {
                self.buffer.put_u8(1);
                self.put_plen(&target.inner);
            }
            Object::Directory => self.buffer.put_u8(2),
        };

        Ok(())
    }

    fn put_plen(&mut self, bytes: &[u8]) {
        self.buffer.put_u64_le(bytes.len() as u64);
        self.buffer.put_slice(bytes);
    }
}

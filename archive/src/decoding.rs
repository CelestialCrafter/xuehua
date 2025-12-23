use alloc::{borrow::Cow, vec};
use core::{num::TryFromIntError, str::Utf8Error};

use blake3::Hash;
use bytes::{Buf, Bytes, TryGetError};
use thiserror::Error;

use crate::{
    Event, Object, Operation,
    hashing::Hasher,
    utils::{State, debug},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected token: {token:?} (expected {expected})")]
    UnexpectedToken {
        token: Bytes,
        expected: Cow<'static, str>,
    },
    #[error("not enough data was in buffer")]
    Incomplete(#[from] TryGetError),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u16),
    #[error("digest mismatch: {found} (expected {expected})")]
    DigestMismatch { expected: Hash, found: Hash },
    #[error(transparent)]
    ConversionError(#[from] TryFromIntError),
    #[error(transparent)]
    Utf8Error(#[from] Utf8Error),
}

pub struct Decoder<'a, B> {
    state: State,
    buffer: &'a mut B,
}

impl<'a, B: Buf> Decoder<'a, B> {
    pub fn new(buffer: &'a mut B) -> Self {
        Self {
            buffer,
            state: Default::default(),
        }
    }

    pub fn with_buffer<'b, T>(self, buffer: &'b mut T) -> Decoder<'b, T> {
        Decoder {
            state: self.state,
            buffer,
        }
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.state.finished()
    }

    pub fn decode(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        core::iter::from_fn(move || {
            if matches!(self.state, State::Operations(0)) {
                None
            } else {
                Some(self.process())
            }
        })
    }

    fn process(&mut self) -> Result<Event, Error> {
        debug!("decoding in state: {:?}", self.state);

        let event = match self.state {
            State::Magic => {
                const MAGIC: &str = "xuehua-archive";
                let token = self.try_copy_to_bytes(MAGIC.len())?;
                if token != MAGIC {
                    return Err(Error::UnexpectedToken {
                        token,
                        expected: MAGIC.into(),
                    });
                }

                let version = self.buffer.try_get_u16_le()?;
                if version != 1 {
                    return Err(Error::UnsupportedVersion(version));
                }

                self.state = State::Index;
                return self.process();
            }
            State::Index => {
                let amount = self.buffer.try_get_u64_le()?.try_into()?;
                let event = Event::Index(
                    (0..amount)
                        .map(|_| {
                            let len = self.buffer.try_get_u64_le()?.try_into()?;
                            let path = self.try_copy_to_bytes(len)?;
                            Ok(path.into())
                        })
                        .collect::<Result<_, Error>>()?,
                );

                self.state = State::Operations(amount);
                event
            }
            State::Operations(..) => {
                let op_type = self.buffer.try_get_u8()?;
                let event = Event::Operation(match op_type {
                    0 => Operation::Create {
                        permissions: self.buffer.try_get_u32_le()?,
                        object: self.get_object()?,
                    },
                    1 => Operation::Delete,
                    _ => {
                        return Err(Error::UnexpectedToken {
                            token: Bytes::from_owner(vec![op_type]),
                            expected: "0 or 1".into(),
                        });
                    }
                });

                match self.state {
                    State::Operations(ref mut amount) => *amount -= 1,
                    _ => unreachable!(),
                }

                event
            }
        };

        debug!("decoded event: {event:?}");
        self.verify_event(&event)?;
        Ok(event)
    }

    fn get_object(&mut self) -> Result<Object, Error> {
        let obj_type = self.buffer.try_get_u8()?;
        debug!("decoding object type {obj_type}");

        Ok(match obj_type {
            0 => Object::File {
                contents: {
                    let len = self.buffer.try_get_u64_le()?.try_into()?;
                    self.try_copy_to_bytes(len)?
                },
            },
            1 => Object::Symlink {
                target: {
                    let len = self.buffer.try_get_u64_le()?.try_into()?;
                    self.try_copy_to_bytes(len)?.into()
                },
            },
            2 => Object::Directory,
            _ => {
                return Err(Error::UnexpectedToken {
                    token: Bytes::from_owner(vec![obj_type]),
                    expected: "0, 1, or 2".into(),
                });
            }
        })
    }

    fn try_copy_to_bytes(&mut self, len: usize) -> Result<Bytes, Error> {
        let remaining = self.buffer.remaining();
        if remaining < len {
            return Err(Error::Incomplete(bytes::TryGetError {
                requested: len,
                available: remaining,
            }));
        } else {
            Ok(self.buffer.copy_to_bytes(len))
        }
    }

    fn get_hash(&mut self) -> Result<Hash, Error> {
        Ok(Hash::from_slice(&self.try_copy_to_bytes(blake3::OUT_LEN)?)
            .expect("bytes should be OUT_LEN long"))
    }

    fn verify_event(&mut self, event: &Event) -> Result<(), Error> {
        let expect = Hasher::hash(event);
        let actual = self.get_hash()?;

        (actual == expect)
            .then_some(())
            .ok_or(Error::DigestMismatch {
                expected: expect,
                found: actual,
            })
    }
}

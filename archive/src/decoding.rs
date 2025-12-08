use alloc::{borrow::Cow, collections::btree_set::BTreeSet, format, vec};
use blake3::{Hash, Hasher};
use core::{num::TryFromIntError, str::Utf8Error};

use bytes::{Buf, Bytes, TryGetError};
use thiserror::Error;

use crate::{
    Contents, Event, Object, Operation,
    utils::{self, State, debug},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected token: {token:?} ({reason})")]
    UnexpectedToken {
        token: Bytes,
        reason: Cow<'static, str>,
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
                self.expect("xuehua-archive")?;

                let version = self.buffer.try_get_u16_le()?;
                if version != 1 {
                    return Err(Error::UnsupportedVersion(version));
                }

                self.state = State::Index;
                self.process()?
            }
            State::Index => {
                let amount = self.buffer.try_get_u64_le()?;
                let mut index = BTreeSet::new();

                let mut hasher = Hasher::new();
                for _ in 0..amount {
                    let len = self.buffer.try_get_u64_le()?.try_into()?;
                    let bytes = self.try_copy_to_bytes(len)?;

                    utils::hash_plen(&mut hasher, &bytes);
                    index.insert(bytes.into());
                }

                self.expect_hash(hasher.finalize())?;
                self.state = State::Operations(index.len());
                Event::Index(index)
            }
            State::Operations(..) => {
                let delete = self.get_bool()?;
                let operation = if delete {
                    Operation::Delete
                } else {
                    Operation::Create {
                        permissions: self.buffer.try_get_u32_le()?,
                        object: self.get_object()?,
                    }
                };

                let hasher = &mut Hasher::new();
                operation.hash(hasher);
                self.expect_hash(hasher.finalize())?;

                match self.state {
                    State::Operations(ref mut amount) => *amount -= 1,
                    _ => unreachable!(),
                }

                Event::Operation(operation)
            }
        };

        debug!("decoded event: {event:?}");

        Ok(event)
    }

    fn get_object(&mut self) -> Result<Object, Error> {
        let obj_type = self.buffer.try_get_u8()?;
        debug!("decoding object type {obj_type}");

        Ok(match obj_type {
            0 => Object::File {
                prefix: {
                    if self.get_bool()? {
                        Some(self.get_hash()?)
                    } else {
                        None
                    }
                },
                contents: Contents::Compressed({
                    let len = self.buffer.try_get_u64_le()?.try_into()?;
                    self.try_copy_to_bytes(len)?
                }),
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
                    reason: "expected 0, 1, or 2".into(),
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

    fn get_bool(&mut self) -> Result<bool, Error> {
        let value = self.buffer.try_get_u8()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::UnexpectedToken {
                token: Bytes::from_owner(vec![value]),
                reason: "0 or 1".into(),
            }),
        }
    }

    fn get_hash(&mut self) -> Result<Hash, Error> {
        Ok(Hash::from_slice(&self.try_copy_to_bytes(blake3::OUT_LEN)?)
            .expect("bytes should be OUT_LEN long"))
    }

    fn expect_hash(&mut self, expect: Hash) -> Result<Hash, Error> {
        let actual = self.get_hash()?;
        if actual == expect {
            Ok(actual)
        } else {
            Err(Error::DigestMismatch {
                expected: expect,
                found: actual,
            })
        }
    }

    fn expect(&mut self, expect: impl Into<Bytes>) -> Result<Bytes, Error> {
        let expect = expect.into();
        let actual = self.try_copy_to_bytes(expect.len())?;
        if actual == expect {
            Ok(actual)
        } else {
            Err(Error::UnexpectedToken {
                token: actual,
                reason: format!("expected {expect:?}").into(),
            })
        }
    }
}

use core::{num::TryFromIntError, str::Utf8Error};
use alloc::{borrow::Cow, collections::VecDeque};

use blake3::Hash;
use bytes::{Buf, Bytes, TryGetError};
use thiserror::Error;

use crate::{
    Event, Object, ObjectMetadata, ObjectType,
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
    metadata: VecDeque<ObjectMetadata>,
    buffer: &'a mut B,
}

impl<'a, B: Buf> Decoder<'a, B> {
    pub fn new(buffer: &'a mut B) -> Self {
        Self {
            buffer,
            state: Default::default(),
            metadata: Default::default(),
        }
    }

    pub fn with_buffer<'b, T>(self, buffer: &'b mut T) -> Decoder<'b, T> {
        Decoder {
            state: self.state,
            metadata: self.metadata,
            buffer,
        }
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.state.finished()
    }

    pub fn decode(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        core::iter::from_fn(move || {
            if let State::Objects(0) = self.state {
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
                let amount = self.buffer.try_get_u64_le()?;
                let (metadata, index) = (0..amount)
                    .map(|_| {
                        let (path, metadata) = self.get_index_entry()?;
                        Ok((metadata, (path, metadata)))
                    })
                    .collect::<Result<_, Error>>()?;

                self.state = State::Objects(amount);
                self.metadata = metadata;

                Event::Index(index)
            }
            State::Objects(..) => {
                let metadata = self
                    .metadata
                    .pop_front()
                    .expect("object count should match index length");
                let event = Event::Object(self.get_object(metadata)?);

                match self.state {
                    State::Objects(ref mut amount) => *amount -= 1,
                    _ => unreachable!(),
                }

                event
            }
        };

        debug!("decoded event: {event:?}");
        self.verify_event(&event)?;
        Ok(event)
    }

    fn get_index_entry(&mut self) -> Result<(crate::PathBytes, ObjectMetadata), Error> {
        let len = self.buffer.try_get_u64_le()?.try_into()?;
        let path = self.try_copy_to_bytes(len)?.into();

        let metadata = ObjectMetadata {
            permissions: self.buffer.try_get_u32_le()?,
            size: self.buffer.try_get_u64_le()?,
            variant: {
                let token = self.buffer.try_get_u8()?;
                match token {
                    0 => ObjectType::File,
                    1 => ObjectType::Symlink,
                    2 => ObjectType::Directory,
                    _ => {
                        return Err(Error::UnexpectedToken {
                            token: vec![token].into(),
                            expected: "0, 1, or 2".into(),
                        });
                    }
                }
            },
        };

        Ok((path, metadata))
    }

    fn get_object(&mut self, metadata: ObjectMetadata) -> Result<Object, Error> {
        debug!("decoding object with metadata {metadata:?}");

        let mut contents = || self.try_copy_to_bytes(metadata.size.try_into()?);
        Ok(match metadata.variant {
            ObjectType::File => Object::File {
                contents: contents()?,
            },
            ObjectType::Symlink => Object::Symlink {
                target: contents()?.into(),
            },
            ObjectType::Directory => Object::Directory,
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

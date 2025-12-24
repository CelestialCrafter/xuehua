//! Decoding of [`Event`]s from binary

use alloc::{borrow::Cow, collections::VecDeque, vec};
use core::num::TryFromIntError;

use blake3::Hash;
use bytes::{Buf, Bytes, TryGetError};
use thiserror::Error;

use crate::{
    Event, Object, ObjectMetadata, ObjectType,
    hashing::Hasher,
    utils::{State, debug},
};

/// Error type for decoding
#[derive(Error, Debug)]
pub enum Error {
    /// An invalid token was provided
    #[error("unexpected token: {token:?} (expected {expected})")]
    UnexpectedToken {
        #[allow(missing_docs)]
        token: Bytes,
        #[allow(missing_docs)]
        expected: Cow<'static, str>,
    },
    /// The buffer did not contain enough data to decode a full event
    #[error("not enough data was in buffer")]
    Incomplete(#[from] TryGetError),
    /// The archive version is unsupported
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u16),
    /// The digest in the archive did not match the decoded [`Event`]'s digest
    #[error("digest mismatch: {found} (expected {expected})")]
    DigestMismatch {
        #[allow(missing_docs)]
        expected: Hash,
        #[allow(missing_docs)]
        found: Hash,
    },
    #[allow(missing_docs)]
    #[error(transparent)]
    ConversionError(#[from] TryFromIntError),
    #[allow(missing_docs)]
    #[cfg(feature = "std")]
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

/// Decoder for archive events
///
/// The decoder consumes [`Bytes`] and outputs [`Event`]s
///
/// A decoder can only decode a single archive.
/// Once [`finished`] returns true, no further data can be decoded.
#[derive(Default)]
pub struct Decoder {
    state: State,
    metadata: VecDeque<ObjectMetadata>,
}

impl Decoder {
    /// Constructs a new encoder
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether or not the decoder has completed
    #[inline]
    pub fn finished(&self) -> bool {
        self.state.finished()
    }

    /// Decodes [`Bytes`] into an iterator of [`Event`]s.
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `buffer` are unmodified,
    /// and this function may be retried.
    pub fn decode(&mut self, buffer: &mut Bytes) -> impl Iterator<Item = Result<Event, Error>> {
        let mut end = false;
        core::iter::from_fn(move || {
            if end || self.finished() {
                None
            } else {
                let mut attempt = buffer.clone();
                Some(
                    self.process(&mut attempt)
                        .inspect(|_| *buffer = attempt)
                        .inspect_err(|_| end = true),
                )
            }
        })
    }

    /// Decodes [`Bytes`] into an iterator of [`Event`]s.
    ///
    /// If possible, use [`decode`] due to this function's compute and memory overhead.
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `reader` are unmodified,
    /// and this function may be retried.
    #[cfg(feature = "std")]
    pub fn decode_reader(
        &mut self,
        reader: &mut impl std::io::Read,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        fn fill_buffer(buffer: &mut Bytes, reader: &mut impl std::io::Read) -> Result<(), Error> {
            debug!("refilling buffer");
            use bytes::BytesMut;

            let mut new = BytesMut::new();
            new.extend_from_slice(buffer);

            let start = new.len();
            new.resize((start * 2).max(4096), 0);
            let mut position = start;

            loop {
                match reader.read(&mut new[position..]) {
                    Ok(0) => {
                        if position > start {
                            new.truncate(position);
                            *buffer = new.freeze();
                            break;
                        }

                        let incomplete = Error::Incomplete(TryGetError {
                            requested: 1,
                            available: 0,
                        });

                        return Err(incomplete.into());
                    }
                    Ok(n) => position += n,
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => (),
                    Err(err) => return Err(err.into()),
                }
            }

            debug!("fetched {} more bytes", position - start);

            Ok(())
        }

        let mut buffer = Bytes::new();
        let mut queue = VecDeque::new();
        std::iter::from_fn(move || {
            Some(loop {
                match queue.pop_front() {
                    None | Some(Err(Error::Incomplete(_))) => {
                        if self.finished() {
                            return None;
                        }

                        match fill_buffer(&mut buffer, reader) {
                            Ok(()) => queue.extend(self.decode(&mut buffer)),
                            Err(err) => break Err(err.into()),
                        }
                    }
                    Some(result) => break result.map_err(Into::into),
                }
            })
        })
    }

    fn process(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
        debug!("decoding in state: {:?}", self.state);

        let event = match self.state {
            State::Magic => {
                const MAGIC: &str = "xuehua-archive";
                let token = try_split_to(buffer, MAGIC.len())?;
                if token != MAGIC {
                    return Err(Error::UnexpectedToken {
                        token,
                        expected: MAGIC.into(),
                    });
                }

                let version = buffer.try_get_u16_le()?;
                if version != 1 {
                    return Err(Error::UnsupportedVersion(version));
                }

                self.state = State::Index;
                return self
                    .process(buffer)
                    .inspect_err(|_| self.state = State::Magic);
            }
            State::Index => {
                let amount = buffer.try_get_u64_le()?;
                let (metadata, index) = (0..usize::try_from(amount)?)
                    .map(|_| {
                        let (path, metadata) = get_index_entry(buffer)?;
                        Ok((metadata, (path, metadata)))
                    })
                    .collect::<Result<_, Error>>()?;

                let event = Event::Index(index);
                verify_event(buffer, &event)?;

                self.state = State::Objects(amount);
                self.metadata = metadata;

                event
            }
            State::Objects(..) => {
                let metadata = self
                    .metadata
                    .front()
                    .expect("object count should match index length");

                let event = Event::Object(get_object(buffer, *metadata)?);
                verify_event(buffer, &event)?;

                if let State::Objects(ref mut amount) = self.state {
                    self.metadata.pop_front();
                    *amount -= 1
                }

                event
            }
        };

        debug!("decoded event: {event:?}");
        Ok(event)
    }
}

fn verify_event(buffer: &mut Bytes, event: &Event) -> Result<(), Error> {
    let expect = Hasher::hash(event);
    let actual = Hash::from_slice(&try_split_to(buffer, blake3::OUT_LEN)?)
        .expect("bytes should be OUT_LEN long");

    (actual == expect)
        .then_some(())
        .ok_or(Error::DigestMismatch {
            expected: expect,
            found: actual,
        })
}

fn get_index_entry(buffer: &mut Bytes) -> Result<(crate::PathBytes, ObjectMetadata), Error> {
    let len = buffer.try_get_u64_le()?.try_into()?;
    let path = try_split_to(buffer, len)?.into();

    let metadata = ObjectMetadata {
        permissions: buffer.try_get_u32_le()?,
        size: buffer.try_get_u64_le()?,
        variant: {
            let token = buffer.try_get_u8()?;
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

fn get_object(buffer: &mut Bytes, metadata: ObjectMetadata) -> Result<Object, Error> {
    debug!("decoding object with metadata {metadata:?}");

    let mut contents = || try_split_to(buffer, metadata.size.try_into()?);
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

fn try_split_to(buffer: &mut Bytes, at: usize) -> Result<Bytes, Error> {
    let len = buffer.len();
    if at > len {
        return Err(Error::Incomplete(bytes::TryGetError {
            requested: at,
            available: len,
        }));
    } else {
        Ok(buffer.split_to(at))
    }
}

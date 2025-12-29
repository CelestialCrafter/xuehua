//! Decoding of [`Event`]s from binary

use alloc::{borrow::Cow, collections::VecDeque};
use core::num::TryFromIntError;

use blake3::Hash;
use bytes::{Buf, Bytes, TryGetError};
use thiserror::Error;

use crate::{
    Object, ObjectContent,
    hashing::Hasher,
    utils::{MAGIC, VERSION, debug},
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
    magic: bool,
}

impl Decoder {
    /// Constructs a new encoder
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes [`Bytes`] into an iterator of [`Event`]s.
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `buffer` are unmodified,
    /// and this function may be retried.
    pub fn decode(&mut self, buffer: &mut Bytes) -> impl Iterator<Item = Result<Object, Error>> {
        core::iter::from_fn(move || {
            let mut attempt = buffer.clone();
            if !self.magic {
                if let Err(err) = process_magic(&mut attempt) {
                    return Some(Err(err));
                }
            }

            let rval = if attempt.is_empty() {
                None
            } else {
                Some(process_object(&mut attempt))
            };

            if let None | Some(Ok(_)) = rval {
                self.magic = true;
                *buffer = attempt;
            }

            rval
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
    ) -> impl Iterator<Item = Result<Object, Error>> {
        fn fill_buffer(
            buffer: &mut Bytes,
            reader: &mut impl std::io::Read,
        ) -> Result<usize, Error> {
            let mut new = bytes::BytesMut::new();
            new.extend_from_slice(buffer);

            let start = new.len();
            let mut position = start;
            new.resize((start * 2).max(4096), 0);

            loop {
                match reader.read(&mut new[position..]) {
                    Ok(0) => {
                        new.truncate(position);
                        *buffer = new.freeze();
                        break;
                    }
                    Ok(n) => position += n,
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => (),
                    Err(err) => return Err(err.into()),
                }
            }

            Ok(position - start)
        }

        let mut buffer = Bytes::new();
        let mut queue = VecDeque::new();
        std::iter::from_fn(move || {
            loop {
                match queue.pop_front() {
                    None | Some(Err(Error::Incomplete(_))) => {
                        match fill_buffer(&mut buffer, reader) {
                            Ok(0) => break None,
                            Ok(_) => {
                                let decoded = self.decode(&mut buffer);
                                let fused = decoded.scan(true, |advance, result| {
                                    advance.then_some(result.inspect_err(|_| *advance = false))
                                });

                                queue.extend(fused)
                            }
                            Err(err) => break Some(Err(err.into())),
                        }
                    }
                    Some(result) => break Some(result.map_err(Into::into)),
                }
            }
        })
    }
}

fn process_magic(buffer: &mut Bytes) -> Result<(), Error> {
    let magic = try_split_to(buffer, MAGIC.len())?;
    if magic != MAGIC {
        return Err(Error::UnexpectedToken {
            token: magic,
            expected: MAGIC.into(),
        });
    }

    let version = buffer.try_get_u16_le()?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }

    debug!("decoded magic {magic:?} and version {version}");
    Ok(())
}

fn process_object(buffer: &mut Bytes) -> Result<Object, Error> {
    let location = process_plen(buffer)?.into();
    let permissions = buffer.try_get_u32_le()?;

    let variant = buffer.try_get_u8()?;
    let content = match variant {
        0 => ObjectContent::File {
            data: process_plen(buffer)?,
        },
        1 => ObjectContent::Symlink {
            target: process_plen(buffer)?.into(),
        },
        2 => ObjectContent::Directory,
        _ => {
            return Err(Error::UnexpectedToken {
                token: Bytes::copy_from_slice(&[variant]),
                expected: "0, 1, or 2".into(),
            });
        }
    };

    let object = Object {
        location,
        permissions,
        content,
    };

    debug!("decoded object: {object:?}");

    let expect = Hasher::hash(&object);
    let actual = Hash::from_slice(&try_split_to(buffer, blake3::OUT_LEN)?)
        .expect("bytes should be OUT_LEN long");

    (actual == expect)
        .then_some(object)
        .ok_or(Error::DigestMismatch {
            expected: expect,
            found: actual,
        })
}

#[inline]
fn process_plen(buffer: &mut Bytes) -> Result<Bytes, Error> {
    let len = buffer.try_get_u64_le()?.try_into()?;
    try_split_to(buffer, len)
}

#[inline]
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

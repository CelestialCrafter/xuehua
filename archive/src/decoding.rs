//! Decoding of [`Event`]s from binary

use alloc::borrow::Cow;
use core::num::TryFromIntError;

use blake3::Hash;
use bytes::{Buf, Bytes, TryGetError};
use ed25519_dalek::Signature;
use thiserror::Error;

use crate::{
    Event, Object, ObjectContent,
    utils::{MAGIC, Marker, VERSION, debug, hash_object},
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
}

/// Decoder for archive events
///
/// The decoder consumes [`Bytes`] and outputs [`Event`]s
///
/// A single decoder can decode multiple archives.
#[derive(Default)]
pub struct Decoder {
    hasher: blake3::Hasher,
}

impl Decoder {
    /// Constructs a new encoder
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Decodes [`Bytes`] into an iterator of [`Event`]s.
    ///
    /// # Errors
    ///
    /// If this function errors, both the internal
    /// state and `buffer` are unmodified,
    /// and this function may be retried.
    #[inline]
    pub fn decode_iter(
        &mut self,
        buffer: &mut Bytes,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        core::iter::from_fn(|| {
            if buffer.is_empty() {
                None
            } else {
                let mut attempt = buffer.clone();
                Some(self.process(&mut attempt).inspect(|_| *buffer = attempt))
            }
        })
    }

    /// Gets the current digest of the archive.
    #[inline]
    pub fn digest(&self) -> blake3::Hash {
        self.hasher.finalize()
    }

    fn process(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
        const PREFIX: &str = "xuehua-archive@";
        let token = try_split_to(buffer, PREFIX.len())?;
        if token != PREFIX {
            return Err(Error::UnexpectedToken {
                token,
                expected: PREFIX.into(),
            });
        }

        let token = try_split_to(buffer, Marker::len())?;
        match token.as_ref() {
            b"hd" => self.process_header(buffer),
            b"ft" => self.process_footer(buffer),
            b"ob" => self.process_object(buffer),
            _ => {
                return Err(Error::UnexpectedToken {
                    token,
                    expected: r#""hd", "ft", or "ob""#.into(),
                });
            }
        }
    }

    fn process_header(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
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

        self.hasher.reset();

        debug!("decoded header with magic {magic:?} and version {version}");
        Ok(Event::Header)
    }

    fn process_footer(&self, buffer: &mut Bytes) -> Result<Event, Error> {
        let hash = self.hasher.finalize();
        verify_hash(buffer, hash)?;

        let amount = buffer.try_get_u64_le()?.try_into()?;
        let signatures = (0..amount)
            .map(|_| {
                let fingerprint = try_get_hash(buffer)?;
                let signature = Signature::from_slice(&try_split_to(buffer, Signature::BYTE_SIZE)?)
                    .expect("bytes should be BYTE_SIZE long");

                Ok((fingerprint, signature))
            })
            .collect::<Result<_, Error>>()?;

        debug!("decoded footer with hash {hash} and signature {signatures:?}");
        Ok(Event::Footer(signatures))
    }

    fn process_object(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
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

        let hash = hash_object(&object);
        verify_hash(buffer, hash)?;
        self.hasher.update(hash.as_bytes());

        Ok(Event::Object(object))
    }
}

fn try_get_hash(buffer: &mut Bytes) -> Result<blake3::Hash, Error> {
    Ok(Hash::from_slice(&try_split_to(buffer, blake3::OUT_LEN)?)
        .expect("bytes should be OUT_LEN long"))
}

fn verify_hash(buffer: &mut Bytes, expected: blake3::Hash) -> Result<(), Error> {
    let found = try_get_hash(buffer)?;
    (found == expected)
        .then_some(())
        .ok_or(Error::DigestMismatch { expected, found })
}

fn process_plen(buffer: &mut Bytes) -> Result<Bytes, Error> {
    let len = buffer.try_get_u64_le()?.try_into()?;
    try_split_to(buffer, len)
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

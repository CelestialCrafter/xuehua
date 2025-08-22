//! Decoding of [`Event`]s from binary

use alloc::borrow::Cow;

use blake3::Hash;
use bytes::{Buf, Bytes};
use ed25519_dalek::Signature;
use xh_reports::prelude::*;

use crate::{
    Event, Object, ObjectContent,
    utils::{ArchiveCompat, MAGIC, Marker, VERSION, debug, hash_object},
};

/// An unexpected token was encountered
#[derive(Debug, IntoReport)]
#[message("unexpected token encountered")]
#[suggestion("provide {expected}")]
#[context(token)]
pub struct UnexpectedTokenError {
    #[allow(missing_docs)]
    token: Bytes,
    #[format(suggestion)]
    #[allow(missing_docs)]
    expected: Cow<'static, str>,
}

/// The archive had an unsupported version
#[derive(Debug, IntoReport)]
#[message("unsupported version")]
#[suggestion("provide version {VERSION}")]
#[context(version)]
pub struct UnsupportedVersionError {
    version: u16,
}

/// An invalid token was provided
/// The archive version is unsupported
/// The digest in the archive did not match the decoded [`Event`]'s digest
#[derive(Debug, IntoReport)]
#[message("digest mismatch: {found} (expected {expected})")]
#[suggestion("provide an event that hashes to {expected}")]
#[context(found)]
pub struct DigestMismatchError {
    #[allow(missing_docs)]
    #[format(message)]
    #[format(suggestion)]
    expected: Hash,
    #[allow(missing_docs)]
    #[format(message)]
    found: Hash,
}

/// Error type for decoding
#[derive(Default, Debug, IntoReport)]
#[message("could not decode archive")]
pub struct Error;

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
            return Err(UnexpectedTokenError {
                token,
                expected: PREFIX.into(),
            }
            .wrap());
        }

        let token = try_split_to(buffer, Marker::len())?;
        match token.as_ref() {
            b"hd" => self.process_header(buffer),
            b"ft" => self.process_footer(buffer),
            b"ob" => self.process_object(buffer),
            _ => {
                return Err(UnexpectedTokenError {
                    token,
                    expected: r#""hd", "ft", or "ob""#.into(),
                }
                .wrap());
            }
        }
    }

    fn process_header(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
        let magic = try_split_to(buffer, MAGIC.len())?;
        if magic != MAGIC {
            return Err(UnexpectedTokenError {
                token: magic,
                expected: MAGIC.into(),
            }
            .wrap());
        }

        let version = buffer.try_get_u16_le().compat().wrap()?;
        if version != VERSION {
            return Err(UnsupportedVersionError { version }.wrap());
        }

        self.hasher.reset();

        debug!("decoded header with magic {magic:?} and version {version}");
        Ok(Event::Header)
    }

    fn process_footer(&self, buffer: &mut Bytes) -> Result<Event, Error> {
        let hash = self.hasher.finalize();
        verify_hash(buffer, hash)?;

        let amount = buffer.try_get_u64_le().compat().wrap()?.try_into().wrap()?;
        let signatures = (0..amount)
            .map(|_| {
                let fingerprint = try_get_hash(buffer)?;
                let signature = Signature::from_slice(&try_split_to(buffer, Signature::BYTE_SIZE)?)
                    .expect("bytes should be BYTE_SIZE long");

                Ok((fingerprint, signature))
            })
            .collect::<Result<_, _>>()?;

        debug!("decoded footer with hash {hash} and signature {signatures:?}");
        Ok(Event::Footer(signatures))
    }

    fn process_object(&mut self, buffer: &mut Bytes) -> Result<Event, Error> {
        let location = process_plen(buffer)?.into();
        let permissions = buffer.try_get_u32_le().compat().wrap()?;

        let variant = buffer.try_get_u8().compat().wrap()?;
        let content = match variant {
            0 => ObjectContent::File {
                data: process_plen(buffer)?,
            },
            1 => ObjectContent::Symlink {
                target: process_plen(buffer)?.into(),
            },
            2 => ObjectContent::Directory,
            _ => {
                return Err(UnexpectedTokenError {
                    token: Bytes::copy_from_slice(&[variant]),
                    expected: "0, 1, or 2".into(),
                }
                .wrap());
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
    try_split_to(buffer, blake3::OUT_LEN)
        .map(|bytes| Hash::from_slice(&bytes).expect("bytes should be OUT_LEN long"))
}

fn verify_hash(buffer: &mut Bytes, expected: blake3::Hash) -> Result<(), Error> {
    let found = try_get_hash(buffer).wrap()?;
    (found == expected)
        .then_some(())
        .ok_or_else(|| DigestMismatchError { expected, found }.wrap())
}

fn process_plen(buffer: &mut Bytes) -> Result<Bytes, Error> {
    let len = buffer
        .try_get_u64_le()
        .compat()
        .wrap()?
        .try_into()
        .wrap()?;
    Ok(try_split_to(buffer, len)?)
}

fn try_split_to(buffer: &mut Bytes, at: usize) -> Result<Bytes, Error> {
    let len = buffer.len();
    if at > len {
        return Err(bytes::TryGetError {
            requested: at,
            available: len,
        })
        .compat()
        .wrap();
    } else {
        Ok(buffer.split_to(at))
    }
}

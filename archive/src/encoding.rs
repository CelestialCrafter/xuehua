//! Encoding of [`Event`]s into binary

use core::borrow::Borrow;

use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    Object, ObjectContent,
    hashing::Hasher,
    utils::{MAGIC, VERSION, debug},
};

/// Encoder for archive events
///
/// The encoder consumes [`Event`]s and outputs binary.
///
/// An encoder can only encode a single archive.
/// Once [`finished`] returns true, no further events can be encoded.
#[derive(Default)]
pub struct Encoder {
    magic: bool,
}

impl Encoder {
    /// Constructs a new encoder.
    #[inline]
    pub fn new() -> Self {
        Default::default()
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
        objects: impl IntoIterator<Item = impl Borrow<Object>>,
    ) {
        if !self.magic {
            buffer.put_slice(MAGIC.as_bytes());
            buffer.put_u16_le(VERSION);
            self.magic = true;
        }

        for object in objects {
            process(buffer, object.borrow());
        }
    }

    /// Encodes an iterator of [`Event`]s into a `writer`
    ///
    /// # Errors
    ///
    /// If this function errors, partial events will not be written.
    #[cfg(feature = "std")]
    #[inline]
    pub fn encode_writer(
        &mut self,
        writer: &mut impl std::io::Write,
        objects: impl IntoIterator<Item = impl Borrow<Object>>,
    ) -> Result<(), std::io::Error> {
        let mut buffer = BytesMut::with_capacity(4096);
        if !self.magic {
            buffer.put_slice(MAGIC.as_bytes());
            buffer.put_u16_le(VERSION);
            writer.write_all(&buffer)?;
            self.magic = true;
        }

        for object in objects {
            buffer.clear();
            process(&mut buffer, object.borrow());
            writer.write_all(&buffer)?;
        }

        Ok(())
    }
}

fn process(buffer: &mut impl BufMut, object: &Object) {
    debug!("encoding object: {object:?}");

    process_lenp(buffer, &object.location.inner);
    buffer.put_u32_le(object.permissions);

    match &object.content {
        ObjectContent::File { data } => {
            buffer.put_u8(0);
            process_lenp(buffer, data);
        }
        ObjectContent::Symlink { target } => {
            buffer.put_u8(1);
            process_lenp(buffer, &target.inner);
        }
        ObjectContent::Directory => {
            buffer.put_u8(2);
        }
    };

    buffer.put_slice(Hasher::hash(object).as_bytes());
}

fn process_lenp(buffer: &mut impl BufMut, bytes: &Bytes) {
    buffer.put_u64_le(bytes.len() as u64);
    buffer.put_slice(bytes);
}

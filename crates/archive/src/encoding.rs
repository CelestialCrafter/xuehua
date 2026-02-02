//! Encoding of [`Event`]s into binary

use std::borrow::Borrow;

use bytes::{BufMut, Bytes};
use ed25519_dalek::Signature;

use crate::{
    Event, Fingerprint, Object, ObjectContent,
    utils::{MAGIC, Marker, VERSION, debug, hash_object},
};

/// Encoder for archive events
///
/// The encoder consumes [`Event`]s and outputs binary data.
#[derive(Clone, Default)]
pub struct Encoder {
    hasher: blake3::Hasher,
}

impl Encoder {
    /// Constructs a new encoder.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Encodes an iterator of [`Event`]s into `buffer`.
    #[inline]
    pub fn encode_iter(
        &mut self,
        buffer: &mut impl BufMut,
        events: impl IntoIterator<Item = impl Borrow<Event>>,
    ) {
        events
            .into_iter()
            .for_each(|event| self.encode(buffer, event))
    }

    /// Encodes a single [`Event`] into `buffer`.
    #[inline]
    pub fn encode(&mut self, buffer: &mut impl BufMut, event: impl Borrow<Event>) {
        match event.borrow() {
            Event::Header => self.process_header(buffer),
            Event::Object(object) => self.process_object(buffer, object),
            Event::Footer(signatures) => self.process_footer(buffer, signatures),
        }
    }

    /// Gets the current digest of the archive.
    #[inline]
    pub fn digest(&self) -> blake3::Hash {
        self.hasher.finalize()
    }

    fn process_header(&mut self, buffer: &mut impl BufMut) {
        debug!("encoding header");
        self.hasher.reset();

        Marker::Header.put(buffer);
        buffer.put_slice(MAGIC.as_bytes());
        buffer.put_u16_le(VERSION);
    }

    fn process_object(&mut self, buffer: &mut impl BufMut, object: &Object) {
        debug!("encoding object: {object:?}");

        Marker::Object.put(buffer);
        Self::process_lenp(buffer, &object.location.inner);
        buffer.put_u32_le(object.permissions);

        match &object.content {
            ObjectContent::File { data } => {
                buffer.put_u8(0);
                Self::process_lenp(buffer, data);
            }
            ObjectContent::Symlink { target } => {
                buffer.put_u8(1);
                Self::process_lenp(buffer, &target.inner);
            }
            ObjectContent::Directory => {
                buffer.put_u8(2);
            }
        };

        let hash = hash_object(object);
        let hash = hash.as_bytes();
        self.hasher.update(hash);
        buffer.put_slice(hash);
    }

    fn process_footer(&self, buffer: &mut impl BufMut, signatures: &Vec<(Fingerprint, Signature)>) {
        Marker::Footer.put(buffer);

        let hash = self.hasher.finalize();
        buffer.put_slice(hash.as_bytes());

        buffer.put_u64_le(signatures.len() as u64);
        for (fingerprint, signature) in signatures {
            buffer.put_slice(fingerprint.as_bytes());
            buffer.put_slice(&signature.to_bytes());
        }

        debug!("encoding footer with hash {hash} and signatures {signatures:?}");
    }

    fn process_lenp(buffer: &mut impl BufMut, bytes: &Bytes) {
        buffer.put_u64_le(bytes.len() as u64);
        buffer.put_slice(bytes);
    }
}

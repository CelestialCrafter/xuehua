//! Hashing of [`Event`]s via [BLAKE3](https://github.com/BLAKE3-team/BLAKE3)

use core::borrow::Borrow;

use bytes::Bytes;

use crate::{Object, ObjectContent, utils::debug};

/// Stateless hashing methods for archives
pub struct Hasher;

impl Hasher {
    /// Hash a single event
    #[inline]
    pub fn hash(event: impl Borrow<Object>) -> blake3::Hash {
        process(event.borrow())
    }

    /// Hash an iterator of hashes
    ///
    /// This is useful for computing the hash of an entire archive.
    #[inline]
    pub fn aggregate(hashes: impl IntoIterator<Item = impl Borrow<blake3::Hash>>) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        hashes.into_iter().for_each(|hash| {
            hasher.update(hash.borrow().as_bytes());
        });
        hasher.finalize()
    }
}

#[inline]
fn process(object: &Object) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();

    process_lenp(&mut hasher, &object.location.inner);
    hasher.update(&object.permissions.to_le_bytes());
    let (variant, content) = match &object.content {
        ObjectContent::File { data } => (0, data),
        ObjectContent::Symlink { target } => (1, &target.inner),
        ObjectContent::Directory => (2, &Bytes::new()),
    };

    hasher.update(&[variant]);
    process_lenp(&mut hasher, content);

    let hash = hasher.finalize();
    debug!("event hashed to {hash}");
    hash
}

#[inline]
fn process_lenp(hasher: &mut blake3::Hasher, bytes: &Bytes) {
    hasher
        .update(&(bytes.len() as u64).to_le_bytes())
        .update(bytes);
}

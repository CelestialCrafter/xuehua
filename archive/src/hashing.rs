//! Hashing of [`Event`]s via [BLAKE3](https://github.com/BLAKE3-team/BLAKE3)

use core::borrow::Borrow;

use crate::{Event, Object, utils::debug};

/// Stateless hashing methods for archives
pub struct Hasher;

impl Hasher {
    /// Hash a single event
    #[inline]
    pub fn hash(event: impl Borrow<Event>) -> blake3::Hash {
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

fn process(event: &Event) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();

    match event {
        Event::Index(index) => {
            index.iter().for_each(|(path, metadata)| {
                hasher
                    .update(&(path.inner.len() as u64).to_le_bytes())
                    .update(&path.inner);

                hasher
                    .update(&metadata.permissions.to_le_bytes())
                    .update(&metadata.size.to_le_bytes())
                    .update(&[metadata.variant as u8]);
            });
        }
        Event::Object(object) => match object {
            Object::File { contents } => {
                hasher.update(&contents);
            }
            Object::Symlink { target } => {
                hasher.update(&target.inner);
            }
            Object::Directory => (),
        },
    }

    let hash = hasher.finalize();
    debug!("event hashed to {hash}");
    hash
}

use core::borrow::Borrow;

use crate::{Event, Object, utils::debug};

pub struct Hasher;

impl Hasher {
    #[inline]
    pub fn hash(event: impl Borrow<Event>) -> blake3::Hash {
        process(event.borrow())
    }

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

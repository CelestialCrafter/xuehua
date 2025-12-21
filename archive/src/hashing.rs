use core::borrow::Borrow;

use bytes::Bytes;

use crate::{Event, Object, Operation, utils::debug};

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
            index
                .iter()
                .for_each(|path| process_plen(&mut hasher, &path.inner));
        }
        Event::Operation(operation) => match operation {
            Operation::Create {
                permissions,
                object,
            } => {
                hasher.update(&[0]);
                hasher.update(&permissions.to_le_bytes());
                match object {
                    Object::File {
                        contents,
                    } => {
                        hasher.update(&[0]);
                        process_plen(&mut hasher, contents);
                    }
                    Object::Symlink { target } => {
                        hasher.update(&[1]);
                        process_plen(&mut hasher, &target.inner);
                    }
                    Object::Directory => {
                        hasher.update(&[2]);
                    }
                }
            }
            Operation::Delete => {
                hasher.update(&[1]);
            }
        },
    }

    let hash = hasher.finalize();
    debug!("event hashed to {hash}");
    hash
}

fn process_plen(hasher: &mut blake3::Hasher, bytes: &Bytes) {
    hasher
        .update(&(bytes.len() as u64).to_le_bytes())
        .update(&bytes);
}

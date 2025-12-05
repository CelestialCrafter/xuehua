#[cfg(feature = "std")]
pub mod filesystem;

pub mod unimplemented {
    pub struct UnimplementedLoader;

    impl super::DictionaryLoader for UnimplementedLoader {
        fn load(&mut self, _id: blake3::Hash) -> Result<bytes::Bytes, super::Error> {
            Err(super::Error::from("dictionary loading unimplemented"))
        }
    }
}

use alloc::boxed::Box;
use blake3::Hash;
use bytes::Bytes;

pub type Error = Box<dyn core::error::Error + Send + Sync>;

pub trait DictionaryLoader {
    fn load(&mut self, id: Hash) -> Result<Bytes, Error>;
}

#[derive(Debug, Clone)]
pub enum Dictionary {
    None,
    Internal(Bytes),
    External(Hash),
}

impl PartialEq for Dictionary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Internal(left), Self::Internal(right)) => left == right,
            (Self::External(left), Self::External(right)) => left == right,
            (Self::None, Self::None) => true,
            (Self::Internal(bytes), Self::External(hash))
            | (Self::External(hash), Self::Internal(bytes)) => blake3::hash(&bytes) == *hash,
            _ => false,
        }
    }
}

impl Eq for Dictionary {}

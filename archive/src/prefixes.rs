#[cfg(feature = "std")]
pub mod filesystem;

pub mod unimplemented {
    pub struct UnimplementedLoader;

    impl super::PrefixLoader for UnimplementedLoader {
        fn load(&mut self, _id: blake3::Hash) -> Result<bytes::Bytes, super::Error> {
            Err(super::Error::from("prefix loading unimplemented"))
        }
    }
}

use alloc::boxed::Box;
use blake3::Hash;
use bytes::Bytes;

pub type Error = Box<dyn core::error::Error + Send + Sync>;

pub trait PrefixLoader {
    fn load(&mut self, id: Hash) -> Result<Bytes, Error>;
}

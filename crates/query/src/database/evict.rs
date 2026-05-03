//! Value eviction (also known as garbage collection)

mod lru;
pub use lru::LRU;

use crate::KeyIndex;

/// Database extension to support value eviction.
pub trait Evict {
    /// Evicts all "garbage" from the database.
    ///
    /// Returns an iterator of all keys evicted.
    fn evict_garbage(&mut self) -> Vec<KeyIndex>;

    /// Evicts an iterator of keys from the database.
    ///
    /// Note that the database may not choose to evict some keys.
    fn evict_iter(&mut self, indicies: impl Iterator<Item = KeyIndex>);
}

/// No-Op eviction extension for when a database does not support eviction.
pub struct NoOp;
impl Evict for NoOp {
    fn evict_garbage(&mut self) -> Vec<KeyIndex> {
        vec![]
    }

    fn evict_iter(&mut self, _indicies: impl Iterator<Item = KeyIndex>) {}
}

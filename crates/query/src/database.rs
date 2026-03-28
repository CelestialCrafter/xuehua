//! Query key, value, and memo storage

use std::{
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    sync::Mutex,
};

use educe::Educe;

use crate::{Key, KeyIndex, Value, store::Memo};

/// Trait for storage of computed values
///
/// Implementors must ensure that the database operates logically
/// (eg. after `set_value`, `value_of` should return Some)
pub trait Database: Send + Sync + 'static {
    /// Keys the database designed to store
    type Key: Key<Value = Self::Value, Database = Self>;
    /// Values the database designed to store
    type Value: Value;

    /// Returns the index or identifier of a given key
    fn index_of(&self, key: &Self::Key) -> KeyIndex;

    /// Returns the key at a given index
    fn key_of(&self, idx: KeyIndex) -> Option<&Self::Key>;

    /// Returns the value at a given index
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    /// Returns the memo at a given index
    fn memo_of(&self, idx: KeyIndex) -> &Memo;

    /// Updates the value at a given index
    fn set_value(&self, idx: KeyIndex, value: Self::Value);
}

/// Simple generic in-memory database
#[derive(Educe)]
#[educe(Default(new, bound(S: Default)))]
pub struct MemoryDatabase<K: Key, S: Default = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    values: Mutex<HashMap<KeyIndex, K::Value, S>>,
    memos: boxcar::Vec<(K, Memo)>,
}

impl<K, S> Database for MemoryDatabase<K, S>
where
    K: Key<Database = Self>,
    S: Default + BuildHasher + Send + Sync + 'static,
{
    type Key = K;
    type Value = K::Value;

    fn index_of(&self, key: &Self::Key) -> KeyIndex {
        let mut lookup = self.lookup.lock().unwrap();
        if let Some(idx) = lookup.get(key).copied() {
            return idx;
        }

        let idx = self.memos.push((key.clone(), Memo::default()));
        let idx = KeyIndex::new::<Self>(idx);
        lookup.insert(key.clone(), idx);

        idx
    }

    fn key_of(&self, idx: KeyIndex) -> Option<&Self::Key> {
        self.memos.get(idx.idx()).map(|(key, _)| key)
    }

    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value> {
        self.values.lock().unwrap().get(&idx).cloned()
    }

    fn memo_of(&self, idx: KeyIndex) -> &Memo {
        self.memos
            .get(idx.idx())
            .map(|(_, memo)| memo)
            .expect("memo should be valid for any given KeyIndex")
    }

    fn set_value(&self, idx: KeyIndex, value: Self::Value) {
        self.values.lock().unwrap().insert(idx, value);
    }
}

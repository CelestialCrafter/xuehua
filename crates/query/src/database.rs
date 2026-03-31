//! Query key, value, and memo storage

use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{BuildHasher, RandomState},
    sync::Mutex,
};

use crate::{Key, KeyIndex, Value};
use educe::Educe;

/// Trait for storage of computed values
///
/// Implementors must ensure that the database operates logically
/// (eg. after `set_value`, `value_of` should return Some)
pub trait Database: Send + Sync + 'static {
    /// Keys the database designed to store
    type Key: Key<Value = Self::Value>;
    /// Values the database designed to store
    type Value: Value;

    /// Returns the index or identifier of a given key
    fn index_of(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex;

    /// Returns the key at a given index
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;

    /// Returns the value at a given index
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    /// Updates the value at a given index
    fn set_value(&self, idx: KeyIndex, value: Self::Value) -> bool;
}

/// Simple generic in-memory database
#[derive(Educe)]
#[educe(Default(new, bound(S: Default)))]
pub struct InMemory<K: Key, S = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<HashMap<KeyIndex, K, S>>,
    values: Mutex<HashMap<KeyIndex, K::Value, S>>,
}

impl<K, S> Database for InMemory<K, S>
where
    K: Key<Database = Self>,
    S: BuildHasher + Send + Sync + 'static,
{
    type Key = K;
    type Value = K::Value;

    fn index_of(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
        let mut lookup = self.lookup.lock().unwrap();
        if let Some(idx) = lookup.get(key).copied() {
            return idx;
        }

        let idx = new();
        let mut keys = self.keys.lock().unwrap();
        lookup.insert(key.clone(), idx);
        keys.insert(idx, key.clone());

        idx
    }

    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.keys.lock().unwrap().get(&idx).cloned()
    }

    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value> {
        self.values.lock().unwrap().get(&idx).cloned()
    }

    fn set_value(&self, idx: KeyIndex, value: Self::Value) -> bool {
        match self.values.lock().unwrap().entry(idx) {
            Entry::Occupied(mut occupied) => {
                let current = occupied.get_mut();
                if *current != value {
                    *current = value;
                    true
                } else {
                    false
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(value);
                true
            }
        }
    }
}

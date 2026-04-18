use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{BuildHasher, RandomState},
    sync::Mutex,
};

use crate::{
    Key, KeyIndex,
    database::{Database, Difference},
};
use educe::Educe;

/// Simple generic in-memory database
#[derive(Educe, Debug)]
#[educe(Default(new, bound(S: Default)))]
pub struct InMemory<K: Key, S = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<HashMap<KeyIndex, K, S>>,
    values: Mutex<HashMap<KeyIndex, K::Value, S>>,
}

impl<K, S> Database for InMemory<K, S>
where
    K: Key,
    K::Value: PartialEq + Clone,
    S: BuildHasher + Send + Sync + 'static,
{
    type Key = K;
    type InputValue = K::Value;
    type OutputValue<'a> = K::Value;

    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
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

    fn key(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.keys.lock().unwrap().get(&idx).cloned()
    }

    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>> {
        self.values.lock().unwrap().get(&idx).cloned()
    }

    fn set_value(&self, idx: KeyIndex, value: Self::InputValue) -> Difference {
        match self.values.lock().unwrap().entry(idx) {
            Entry::Occupied(mut occupied) => {
                let current = occupied.get_mut();
                if *current != value {
                    *current = value;
                    Difference::Changed
                } else {
                    Difference::Unchanged
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(value);
                Difference::Changed
            }
        }
    }
}

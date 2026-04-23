use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{BuildHasher, Hash, RandomState},
    sync::Mutex,
};

use crate::{
    KeyIndex,
    database::{Database, Difference},
};
use educe::Educe;
use rustc_hash::FxHashMap;

/// Simple generic in-memory database
#[derive(Educe, Debug)]
#[educe(Default(new, bound(S: Default)))]
pub struct InMemory<K, V, S = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<FxHashMap<KeyIndex, K>>,
    values: Mutex<FxHashMap<KeyIndex, V>>,
}

impl<K, V, S> Database for InMemory<K, V, S>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Eq + Clone + Send + Sync + 'static,
    S: BuildHasher + Send + Sync + 'static,
{
    type Key = K;
    type InputValue = V;
    type OutputValue<'a> = V;

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

    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        let diff = self.set_value(idx, value.clone());
        (value, diff)
    }

    fn evict_iter(&mut self, indicies: impl Iterator<Item = KeyIndex>) {
        let lookup = self.lookup.get_mut().unwrap();
        let keys = self.keys.get_mut().unwrap();
        let values = self.values.get_mut().unwrap();

        for idx in indicies {
            values.remove(&idx);
            if let Some(key) = keys.remove(&idx) {
                lookup.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{KeyIndex, database::{Database, InMemory}};

    #[test]
    fn test_eviction() {
        let mut database = InMemory::<(), usize>::new();

        let idx = database.index(&(), || KeyIndex(0));
        database.set_value(idx, 0);

        database.evict_iter(std::iter::once(idx));
        assert_eq!(database.key(idx), None);
        assert_eq!(database.value(idx), None);
    }
}

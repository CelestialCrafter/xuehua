use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{BuildHasher, Hash},
    sync::Mutex,
};

use crate::{
    KeyIndex,
    database::{Database, Difference, evict::Evict, persist},
};
use educe::Educe;
use rapidhash::{RapidHashMap, fast::RandomState};

/// Simple generic in-memory database
#[derive(Educe, Debug)]
#[educe(Default(new, bound(S: Default)))]
pub struct InMemory<K, V, S = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<RapidHashMap<KeyIndex, K>>,
    values: Mutex<RapidHashMap<KeyIndex, V>>,
    persist: persist::NoOp<V>,
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
    type PersistExtension<'a> = persist::NoOp<V>;
    type EvictionExtension<'a> = Self;

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

    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        let mut values = self.values.lock().unwrap();
        let diff = match values.entry(idx) {
            Entry::Occupied(mut occupied) => {
                let current = occupied.get_mut();
                if *current != value {
                    *current = value.clone();
                    Difference::Changed
                } else {
                    Difference::Unchanged
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(value.clone());
                Difference::Changed
            }
        };

        (value, diff)
    }

    fn persistence(&self) -> &Self::PersistExtension<'_> {
        &self.persist
    }

    fn eviction(&mut self) -> &mut Self::EvictionExtension<'_> {
        self
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> Evict for InMemory<K, V, S> {
    fn evict_garbage(&mut self) -> Vec<KeyIndex> {
        vec![]
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
    use crate::{
        KeyIndex,
        database::{Database, InMemory, evict::Evict},
    };

    #[test]
    fn test_eviction() {
        let mut database = InMemory::<(), usize>::new();
        let idx = database.index(&(), || KeyIndex(0));

        database.pass_value(idx, 0);
        database.eviction().evict_iter(std::iter::once(idx));

        assert_eq!(database.key(idx), None);
        assert_eq!(database.value(idx), None);
    }
}

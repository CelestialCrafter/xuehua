use std::sync::{Mutex, atomic::AtomicUsize};

use educe::Educe;
use rapidhash::RapidHashMap;

use crate::{
    KeyIndex,
    database::{Database, Difference, evict::Evict},
};

pub const DEFAULT_CAPACITY: usize = 512;

/// Database adapter using Least Recently Used (LRU) as an eviction policy
#[derive(Debug, Educe)]
#[educe(Default(new))]
pub struct LRU<D> {
    inner: D,
    #[educe(Default(expr = DEFAULT_CAPACITY))]
    capacity: usize,
    counter: AtomicUsize,
    usage: Mutex<RapidHashMap<KeyIndex, usize>>,
}

impl<D: Database> LRU<D> {
    /// Modifies the cache's capacity
    pub fn resize(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    fn update(&self, idx: KeyIndex) {
        let stamp = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut usage = self.usage.lock().unwrap();
        if usage.capacity() == 0 {
            usage.reserve(self.capacity);
        }

        *usage.entry(idx).or_default() = stamp;
    }
}

impl<D: Database> Database for LRU<D> {
    type Key = D::Key;
    type InputValue = D::InputValue;
    type OutputValue<'a> = D::OutputValue<'a>;
    type EvictionExtension<'a> = Self;
    type PersistExtension<'a> = D::PersistExtension<'a>;

    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
        self.inner.index(key, new)
    }

    fn key(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.inner.key(idx)
    }

    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>> {
        self.update(idx);
        self.inner.value(idx)
    }

    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        self.update(idx);
        self.inner.pass_value(idx, value)
    }

    fn eviction(&mut self) -> &mut Self::EvictionExtension<'_> {
        self
    }

    fn persistence(&self) -> &Self::PersistExtension<'_> {
        self.inner.persistence()
    }
}

impl<D: Database> Evict for LRU<D> {
    fn evict_garbage(&mut self) -> Vec<KeyIndex> {
        let usage = self.usage.get_mut().unwrap();

        let evict_count = usage.len().saturating_sub(self.capacity);
        if evict_count == 0 {
            return vec![];
        }

        let mut stamps: Vec<usize> = usage.values().copied().collect();
        let (_, &mut threshold, _) = stamps.select_nth_unstable(evict_count - 1);

        let mut evicted = Vec::with_capacity(evict_count);
        usage.retain(|&idx, &mut stamp| {
            if stamp <= threshold {
                evicted.push(idx);
                false
            } else {
                true
            }
        });

        self.inner.eviction().evict_iter(evicted.iter().copied());
        evicted
    }

    fn evict_iter(&mut self, indicies: impl Iterator<Item = KeyIndex>) {
        self.inner.eviction().evict_iter(indicies);
    }
}

#[cfg(all(test, feature = "inventory"))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        Query,
        database,
        engine::{Context, Engine},
    };

    use super::{DEFAULT_CAPACITY, LRU};

    #[tokio::test]
    async fn test_eviction() {
        static LRU_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(LRU<database::Default<LRUQuery, ()>>)]
        #[compute(Self::inner)]
        struct LRUQuery(usize);
        impl LRUQuery {
            async fn inner(self, _qcx: &Context<'_>) -> <Self as Query>::Value {
                LRU_COMPUTES.fetch_add(1, Ordering::Relaxed);
            }
        }

        let mut engine = Engine::new();
        let fill = async |engine: &mut Engine| {
            for i in 0..DEFAULT_CAPACITY * 2 {
                engine.context().query(LRUQuery(i)).await;
            }
        };

        fill(&mut engine).await;
        // allow the engine to evict 0..DEFAULT_CAPACITY
        engine.upcoming();
        fill(&mut engine).await;

        assert_eq!(LRU_COMPUTES.load(Ordering::Relaxed), DEFAULT_CAPACITY * 3);
    }
}

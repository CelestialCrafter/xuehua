use std::{
    fmt::Debug,
    sync::{Arc, Mutex, atomic::Ordering},
};

use educe::Educe;
use tokio::task::{JoinError, JoinSet};

use crate::{
    Key, KeyIndex,
    store::{Database, Store, VerificationResult},
};

#[derive(Debug)]
pub struct Root {
    store: Arc<Store>,
}

impl Root {
    pub fn new() -> Self {
        Self {
            store: Default::default(),
        }
    }

    pub fn borrowed(&self) -> Borrowed<'_> {
        Borrowed {
            store: &self.store,
            dependencies: Default::default(),
        }
    }

    fn store_mut(&mut self) -> &mut Store {
        Arc::get_mut(&mut self.store).expect("store should not have outstanding references")
    }

    pub fn upcoming(&mut self) -> Upcoming<'_> {
        let store = self.store_mut();
        store.revision = store
            .revision
            .checked_add(1)
            .expect("revision should not exceed NonZeroUsize::MAX");
        Upcoming { store }
    }

    pub fn register<K: Key>(mut self, database: K::Database) -> Self {
        let store = self.store_mut();
        store.register(database);
        self
    }
}

#[derive(Debug)]
pub struct Upcoming<'a> {
    store: &'a mut Store,
}

impl Upcoming<'_> {
    pub fn update<K: Key>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, key);
        database.update_value(idx, value);

        let memo = self
            .store
            .memos
            .get_mut(idx.0)
            .expect("memo should be valid for any KeyIndex");

        memo.verified_at = self.store.revision.get().into();
        memo.dependencies = Default::default();
    }
}

async fn query_epilogue<K: Key>(key: K, idx: KeyIndex, handle: Borrowed<'_>) -> K::Value {
    let value = key.compute(&handle).await;

    let store = handle.store;
    let database = store.database_of::<K>();
    database.update_value(idx, value.clone());

    let memo = &store.memos[idx.0];
    memo.verified_at
        .store(store.revision.get(), Ordering::Release);

    let mut dependencies = memo.dependencies.lock().unwrap();
    *dependencies = handle.dependencies.into_inner().unwrap();

    value
}

#[derive(Debug)]
pub struct Borrowed<'a> {
    store: &'a Arc<Store>,
    dependencies: Mutex<Vec<KeyIndex>>,
}

impl Borrowed<'_> {
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().push(idx);

        if let VerificationResult::Cached { value } = self.store.verify(database, idx) {
            return value;
        };

        query_epilogue(
            key,
            idx,
            Borrowed {
                store: &self.store,
                dependencies: Default::default(),
            },
        )
        .await
    }
}

#[derive(Educe)]
#[educe(Debug)]
pub struct QuerySet<'a, K: Key> {
    handle: &'a Borrowed<'a>,
    cached: Vec<K::Value>,
    joinset: JoinSet<K::Value>,
}

impl<'a, K: Key> QuerySet<'a, K> {
    pub fn new(handle: &'a Borrowed<'a>) -> Self {
        Self {
            handle,
            cached: Default::default(),
            joinset: Default::default(),
        }
    }

    pub fn spawn(&mut self, key: K) {
        let store = self.handle.store;
        let database = store.database_of::<K>();
        let idx = store.index_of(database, &key);
        self.handle.dependencies.lock().unwrap().push(idx);

        match store.verify(database, idx) {
            VerificationResult::Cached { value } => self.cached.push(value),
            VerificationResult::Outdated => {
                let store = store.clone();
                self.joinset.spawn(async move {
                    let handle = Borrowed {
                        store: &store,
                        dependencies: Default::default(),
                    };

                    query_epilogue(key, idx, handle).await
                });
            }
        };
    }

    pub async fn try_next(&mut self) -> Option<Result<K::Value, JoinError>> {
        match self.cached.pop() {
            Some(value) => return Some(Ok(value)),
            None => self.joinset.join_next().await,
        }
    }

    pub async fn next(&mut self) -> Option<K::Value> {
        self.try_next()
            .await
            .map(|result| result.expect("QuerySet::next() should return Ok"))
    }

    pub async fn try_all<T: Default + Extend<Result<K::Value, JoinError>>>(&mut self) -> T {
        let mut collection = T::default();
        collection.extend(self.cached.drain(..).map(Ok));

        while let Some(result) = self.joinset.join_next().await {
            collection.extend(std::iter::once(result));
        }

        collection
    }

    pub async fn all<T: Default + Extend<K::Value>>(&mut self) -> T {
        let mut collection = T::default();
        collection.extend(self.cached.drain(..));

        while let Some(result) = self.joinset.join_next().await {
            collection.extend(std::iter::once(
                result.expect("JoinSet::next() should return Ok"),
            ));
        }

        collection
    }
}

use std::{
    fmt::Debug,
    sync::{Arc, Mutex, atomic::Ordering},
};

use rustc_hash::FxHashSet;

use crate::{
    Key, KeyIndex,
    store::{Database, Store},
};

#[derive(Debug)]
pub struct Root {
    store: Arc<Store>,
}

impl Default for Root {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    pub fn new() -> Self {
        Self {
            store: Arc::default(),
        }
    }

    pub fn handle(&self) -> Handle<'_> {
        Handle {
            store: &self.store,
            dependencies: Mutex::default(),
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

    pub fn register_default<K>(self) -> Self
    where
        K: Key,
        K::Database: Default,
    {
        self.register::<K>(K::Database::default())
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
        database.set_value(idx, value);

        let memo = self
            .store
            .memos
            .get_mut(idx.0)
            .expect("memo should be valid for any KeyIndex");

        let revision = self.store.revision.get();
        *memo.verified_at.get_mut() = revision;
        *memo.changed_at.get_mut() = revision;
        memo.dependencies = Mutex::default();
    }
}

#[derive(Debug)]
pub struct Handle<'a> {
    pub(crate) store: &'a Arc<Store>,
    pub(crate) dependencies: Mutex<FxHashSet<KeyIndex>>,
}

impl Handle<'_> {
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().insert(idx);

        self.store.update(idx).await;
        database
            .value_of(idx)
            .expect("value should exist in database")
    }

    pub(crate) async fn query_inner<K: Key>(
        &self,
        database: &K::Database,
        key: K,
        idx: KeyIndex,
    ) -> bool {
        let child = Handle {
            store: self.store,
            dependencies: Mutex::default(),
        };

        let old = database.value_of(idx);
        let new = key.compute(&child).await;

        let revision = self.store.revision.get();
        let memo = &self.store.memos[idx.0];

        let changed = if old.is_some_and(|old| old == new) {
            false
        } else {
            database.set_value(idx, new);
            memo.changed_at.store(revision, Ordering::Release);

            true
        };

        let mut deps = memo.dependencies.lock().unwrap();
        *deps = child.dependencies.into_inner().unwrap();

        memo.verified_at.store(revision, Ordering::Release);
        changed
    }
}

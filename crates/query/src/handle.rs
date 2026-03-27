//! Engine accessors and action execution

use std::{
    fmt::Debug,
    sync::{Arc, Mutex, atomic::Ordering},
};

use educe::Educe;
use rustc_hash::FxHashSet;

use crate::{
    Key, KeyIndex,
    store::{Database, Store},
};

/// This handle owns the engine, and loans out [`Upcoming`] and [`Handle`]s to utilize it.
#[derive(Debug, Educe)]
#[educe(Default(new))]
pub struct Root {
    store: Arc<Store>,
}

impl Root {
    /// Loan out a [`Handle`] to query the engine
    pub fn handle(&self) -> Handle<'_> {
        Handle {
            store: &self.store,
            dependencies: Mutex::default(),
        }
    }

    fn store_mut(&mut self) -> &mut Store {
        Arc::get_mut(&mut self.store).expect("store should not have outstanding references")
    }

    /// Loan out an [`Upcoming`] to mutate the engine
    pub fn upcoming(&mut self) -> Upcoming<'_> {
        let store = self.store_mut();
        store.revision = store
            .revision
            .checked_add(1)
            .expect("revision should not exceed NonZeroUsize::MAX");
        Upcoming { store }
    }

    /// Helper function for databases that implement [`Default`]
    pub fn register_default<K>(self) -> Self
    where
        K: Key,
        K::Database: Default,
    {
        self.register(K::Database::default())
    }

    /// Registers a database into the engine
    pub fn register(mut self, database: impl Database) -> Self {
        let store = self.store_mut();
        store.register(database);
        self
    }
}

/// Handle to mutate the values for an upcoming revision
#[derive(Debug)]
pub struct Upcoming<'a> {
    store: &'a mut Store,
}

impl Upcoming<'_> {
    /// Update the value for any given key
    pub fn update<K: Key>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = database.index_of(key);
        database.set_value(idx, value);

        let memo = database.memo_of(idx);
        let revision = self.store.revision.get();

        memo.verified_at.store(revision, Ordering::Release);
        memo.changed_at.store(revision, Ordering::Release);
        *memo.dependencies.lock().unwrap() = FxHashSet::default();
    }
}

/// Handle to the current revision
#[derive(Debug)]
pub struct Handle<'a> {
    pub(crate) store: &'a Arc<Store>,
    pub(crate) dependencies: Mutex<FxHashSet<KeyIndex>>,
}

impl Handle<'_> {
    /// Queries the engine for the memoized value computed from `key`
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = database.index_of(&key);
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
        let memo = database.memo_of(idx);

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

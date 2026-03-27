//! Storage for query data

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    hash::{BuildHasher, RandomState},
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use futures_util::{FutureExt, future::BoxFuture};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::{sync::Semaphore, task::JoinSet};

use crate::{Key, KeyIndex, Value, handle::Handle};

#[derive(Debug, Educe)]
#[educe(Default)]
pub struct Memo {
    pub(crate) verified_at: AtomicUsize,
    pub(crate) changed_at: AtomicUsize,
    pub(crate) dependencies: Mutex<FxHashSet<KeyIndex>>,
    #[educe(Default(expr = Semaphore::new(1)))]
    computing: Semaphore,
}

#[derive(Educe, Debug)]
#[educe(Default)]
pub struct Store {
    // NOTE: potentially store memo next to the database
    databases: FxHashMap<TypeId, Box<dyn DynDatabase>>,
    pub memos: boxcar::Vec<Memo>,
    // index_of depends on revision 0 being non-existent
    #[educe(Default = NonZeroUsize::new(1).unwrap())]
    pub revision: NonZeroUsize,
}

trait DynDatabase: Any + Send + Sync {
    fn query<'a>(&'a self, handle: &'a Handle<'_>, idx: KeyIndex) -> BoxFuture<'a, bool>;
}

impl<T: Database> DynDatabase for T {
    fn query<'a>(&'a self, handle: &'a Handle<'_>, idx: KeyIndex) -> BoxFuture<'a, bool> {
        let key = self.key_of(idx).expect("key should exist");
        handle.query_inner(self, key, idx).boxed()
    }
}

impl fmt::Debug for Box<dyn DynDatabase> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self as &dyn Any).fmt(f)
    }
}

impl Store {
    pub fn database_of<K: Key>(&self) -> &K::Database {
        let database = self
            .databases
            .get(&TypeId::of::<K::Database>())
            .expect("database should be registered");

        (database.as_ref() as &dyn Any)
            .downcast_ref::<K::Database>()
            .expect("database should be of type K::Database")
    }

    pub fn index_of<D: Database>(&self, database: &D, key: &D::Key) -> KeyIndex {
        database.index_of(key, || {
            let idx = self.memos.push(Memo {
                verified_at: 0.into(),
                changed_at: 0.into(),
                dependencies: Mutex::default(),
                computing: Semaphore::new(1),
            });

            KeyIndex(idx, database.type_id())
        })
    }

    pub async fn update(self: &Arc<Self>, idx: KeyIndex) -> usize {
        let revision = self.revision.get();

        let memo = &self.memos[idx.0];
        let load_changed_at = || memo.changed_at.load(Ordering::Acquire);
        macro_rules! load_va {
            () => {{
                let verified_at = memo.verified_at.load(Ordering::Acquire);
                if verified_at == revision {
                    return load_changed_at();
                }

                verified_at
            }};
        }

        // short circut if the memo is initially verified
        load_va!();

        let _permit = memo
            .computing
            .acquire()
            .await
            .expect("permit should not be closed");

        // short circut if someone else verified the memo while we were waiting
        let verified_at = load_va!();

        let mut recompute = verified_at == 0;
        let dependencies = memo.dependencies.lock().unwrap().clone();
        if !dependencies.is_empty() {
            let mut joinset = JoinSet::new();
            for dep in dependencies {
                let store = self.clone();
                joinset.spawn(async move { store.update_inner(dep).await });
            }

            while let Some(res) = joinset.join_next().await {
                let dep_ca = res.expect("dependency query should not panic");
                recompute |= dep_ca > verified_at;
            }
        }

        let changed_at = if recompute {
            let database = &self.databases[&idx.1];
            let handle = Handle {
                store: self,
                dependencies: Mutex::default(),
            };

            let changed = database.query(&handle, idx).await;
            changed.then_some(revision)
        } else {
            None
        };

        memo.verified_at.store(revision, Ordering::Release);
        changed_at.unwrap_or_else(load_changed_at)
    }

    fn update_inner(self: &Arc<Self>, idx: KeyIndex) -> BoxFuture<'_, usize> {
        self.update(idx).boxed()
    }

    pub fn register(&mut self, database: impl Database) {
        self.databases
            .entry(database.type_id())
            .or_insert_with(|| Box::new(database));
    }
}

/// Trait for storage of computed values
///
/// Implementors must ensure that the database operates logically
/// (eg. after set_value, value_of should return Some)
pub trait Database: Send + Sync + 'static {
    /// Keys the database designed to store
    type Key: Key<Value = Self::Value, Database = Self>;
    /// Values the database designed to store
    type Value: Value;

    /// Returns the index or identifier of a given key
    fn index_of(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex;
    /// Returns the key at a given index
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;

    /// Returns the Value at a given index
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    /// Updates the value at a given index
    fn set_value(&self, idx: KeyIndex, value: Self::Value);
}

/// Simple generic in-memory database
#[derive(Educe)]
#[educe(Default(new, bound(S: Default)))]
pub struct MemoryDatabase<K: Key, S: Default = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<HashMap<KeyIndex, K, S>>,
    values: Mutex<HashMap<KeyIndex, K::Value, S>>,
}

impl<K: Key<Database = Self>, S: Default + BuildHasher + Send + Sync + 'static> Database
    for MemoryDatabase<K, S>
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

    fn set_value(&self, idx: KeyIndex, value: Self::Value) {
        self.values.lock().unwrap().insert(idx, value);
    }
}

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    sync::{
        RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rustc_hash::FxHashMap;

use crate::{Key, KeyIndex};

#[derive(Debug)]
pub(crate) struct Memo {
    pub verified_at: AtomicUsize,
    pub dependencies: Vec<KeyIndex>,
}

#[derive(Default, Educe)]
#[educe(Debug)]
pub(crate) struct Store {
    index: AtomicUsize,
    #[educe(Debug(ignore))]
    pub databases: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
    pub memos: RwLock<FxHashMap<KeyIndex, Memo>>,
    pub revision: usize,
}

impl Store {
    pub fn database_of<K: Key>(&self) -> &K::Database {
        self.databases[&TypeId::of::<K::Database>()]
            .downcast_ref::<K::Database>()
            .expect("database should be of type K::Database")
    }

    pub fn index_of<D>(&self, database: &D, key: &D::Key) -> KeyIndex
    where
        D: Database,
        D::Key: Key,
    {
        database.index_of(key).unwrap_or_else(|| {
            let idx = KeyIndex(self.index.fetch_add(1, Ordering::Relaxed));
            database.store_key(idx, key.clone());
            idx
        })
    }

    pub fn verify(&self, idx: KeyIndex) -> bool {
        fn inner(
            memos: &FxHashMap<KeyIndex, Memo>,
            idx: KeyIndex,
            revision: usize,
            parent_revision: Option<usize>,
        ) -> bool {
            let Some(memo) = memos.get(&idx) else {
                return false;
            };

            let verified_at = memo.verified_at.load(Ordering::Relaxed);

            // if our parent was verified before us, they're invalid
            if let Some(parent_revision) = parent_revision
                && parent_revision < verified_at
            {
                return false;
            }

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == revision {
                return true;
            }

            // cold path, deep verify dependencies
            for dep_idx in &memo.dependencies {
                if !inner(memos, *dep_idx, revision, Some(verified_at)) {
                    return false;
                }
            }

            memo.verified_at.store(revision, Ordering::Relaxed);
            true
        }

        inner(&self.memos.read().unwrap(), idx, self.revision, None)
    }
}

pub trait Database: Send + Sync + 'static {
    type Key;
    type Value;

    fn index_of(&self, key: &Self::Key) -> Option<KeyIndex>;
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    fn store_key(&self, idx: KeyIndex, key: Self::Key);
    fn store_value(&self, idx: KeyIndex, memo: Self::Value);
}

#[derive(Educe)]
#[educe(Default(bound(S: Default)))]
pub struct MemoryDatabase<K: Key, S = RandomState> {
    lookup: RwLock<HashMap<K, KeyIndex, S>>,
    keys: RwLock<HashMap<KeyIndex, K, S>>,
    values: RwLock<HashMap<KeyIndex, K::Value, S>>,
}

impl<K: Key, S: BuildHasher + Send + Sync + 'static> Database for MemoryDatabase<K, S> {
    type Key = K;
    type Value = K::Value;

    fn index_of(&self, key: &Self::Key) -> Option<KeyIndex> {
        self.lookup.read().unwrap().get(key).copied()
    }

    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.keys.read().unwrap().get(&idx).cloned()
    }

    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value> {
        self.values.read().unwrap().get(&idx).cloned()
    }

    fn store_key(&self, idx: KeyIndex, key: Self::Key) {
        self.lookup.write().unwrap().insert(key.clone(), idx);
        self.keys.write().unwrap().insert(idx, key);
    }

    fn store_value(&self, idx: KeyIndex, value: Self::Value) {
        self.values.write().unwrap().insert(idx, value);
    }
}

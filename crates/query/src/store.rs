use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rustc_hash::FxHashMap;
use tokio::sync::Mutex as AsyncMutex;

use crate::{Key, KeyIndex, Value};

#[derive(Debug)]
pub(crate) struct Memo {
    pub verified_at: AtomicUsize,
    pub dependencies: Vec<KeyIndex>,
    pub computing: Arc<AsyncMutex<()>>,
}

#[derive(Default, Debug)]
pub(crate) struct Store {
    index: Mutex<usize>,
    databases: RwLock<FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    pub memos: RwLock<FxHashMap<KeyIndex, Memo>>,
    pub revision: usize,
}

pub(crate) enum VerificationResult<D: Database> {
    Cached { value: D::Value },
    Outdated { compute_lock: Arc<AsyncMutex<()>> },
    New,
}

impl Store {
    pub fn database_of<K: Key>(&self) -> Arc<K::Database> {
        let type_id = TypeId::of::<K::Database>();
        let database = self.databases.read().unwrap().get(&type_id).cloned();
        database
            .map(|any| {
                any.downcast::<K::Database>()
                    .expect("database should be of type K::Database")
            })
            .unwrap_or_else(|| {
                let database = Arc::new(K::Database::default());
                self.databases
                    .write()
                    .unwrap()
                    .insert(type_id, database.clone());
                database
            })
    }

    pub fn index_of<D: Database>(&self, database: &D, key: &D::Key) -> KeyIndex {
        database.index_of(key).unwrap_or_else(|| {
            let mut current_index = self.index.lock().unwrap();
            *current_index += 1;

            let idx = KeyIndex(*current_index);
            database.store_key(idx, key.clone());
            idx
        })
    }

    pub fn verify<'a, D: Database>(&self, database: &D, idx: KeyIndex) -> VerificationResult<D> {
        fn inner(
            store: &Store,
            memos: &FxHashMap<KeyIndex, Memo>,
            idx: KeyIndex,
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
            if verified_at == store.revision {
                return true;
            }

            // cold path, deep verify dependencies
            for dep_idx in &memo.dependencies {
                if !inner(store, memos, *dep_idx, Some(verified_at)) {
                    return false;
                }
            }

            memo.verified_at.store(store.revision, Ordering::Relaxed);
            true
        }

        let memos = self.memos.read().unwrap();
        if inner(self, &memos, idx, None)
            && let Some(value) = database.value_of(idx)
        {
            VerificationResult::Cached { value }
        } else if let Some(memo) = memos.get(&idx) {
            VerificationResult::Outdated {
                compute_lock: memo.computing.clone(),
            }
        } else {
            VerificationResult::New
        }
    }
}

pub trait Database: Default + Send + Sync + 'static {
    type Key: Key<Value = Self::Value>;
    type Value: Value;

    fn index_of(&self, key: &Self::Key) -> Option<KeyIndex>;
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    fn store_key(&self, idx: KeyIndex, key: Self::Key);
    fn store_value(&self, idx: KeyIndex, memo: Self::Value);
}

#[derive(Educe)]
#[educe(Default(new, bound(S: Default)))]
pub struct MemoryDatabase<K: Key, S: Default = RandomState> {
    lookup: RwLock<HashMap<K, KeyIndex, S>>,
    keys: RwLock<HashMap<KeyIndex, K, S>>,
    values: RwLock<HashMap<KeyIndex, K::Value, S>>,
}

impl<K: Key, S: Default + BuildHasher + Send + Sync + 'static> Database for MemoryDatabase<K, S> {
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
        if let (Ok(mut lookup), Ok(mut keys)) = (self.lookup.write(), self.keys.write()) {
            lookup.insert(key.clone(), idx);
            keys.insert(idx, key);
        }
    }

    fn store_value(&self, idx: KeyIndex, value: Self::Value) {
        if let Ok(mut values) = self.values.write() {
            values.insert(idx, value);
        }
    }
}

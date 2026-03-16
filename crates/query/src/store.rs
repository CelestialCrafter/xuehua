use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    num::NonZeroUsize,
    sync::{
        Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rustc_hash::FxHashMap;

use crate::{Key, KeyIndex, Value};

#[derive(Debug)]
pub(crate) struct Memo {
    pub verified_at: AtomicUsize,
    pub dependencies: Mutex<Vec<KeyIndex>>,
}

#[derive(Educe, Debug)]
#[educe(Default)]
pub(crate) struct Store {
    databases: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
    pub memos: boxcar::Vec<Memo>,
    // some things depend on revision 0 being non-existent
    #[educe(Default = NonZeroUsize::new(1).unwrap())]
    pub revision: NonZeroUsize,
}

pub(crate) enum VerificationResult<D: Database> {
    Cached { value: D::Value },
    Outdated,
}

impl Store {
    pub fn database_of<K: Key>(&self) -> &K::Database {
        let database = self
            .databases
            .get(&TypeId::of::<K::Database>())
            .expect("database should be registered");
        database
            .downcast_ref::<K::Database>()
            .expect("database should be of type K::Database")
    }

    pub fn index_of<D: Database>(&self, database: &D, key: &D::Key) -> KeyIndex {
        database.index_of(key).unwrap_or_else(|| {
            let idx = KeyIndex(self.memos.push(Memo {
                verified_at: (self.revision.get() - 1).into(),
                dependencies: Default::default(),
            }));

            database.store_key(idx, key.clone());
            idx
        })
    }

    pub fn verify<D: Database>(&self, database: &D, idx: KeyIndex) -> VerificationResult<D> {
        #[derive(Debug)]
        enum Operation {
            Validate { parent_revision: usize },
            Update,
        }

        let mut queue = vec![(
            &self.memos[idx.0],
            Operation::Validate {
                parent_revision: usize::MAX,
            },
        )];

        let valid = loop {
            let Some((memo, entry)) = queue.pop() else {
                break true;
            };

            let (verified_at, parent_revision) = match entry {
                Operation::Validate { parent_revision } => {
                    (memo.verified_at.load(Ordering::Relaxed), parent_revision)
                }
                Operation::Update => {
                    memo.verified_at
                        .store(self.revision.get(), Ordering::Relaxed);
                    continue;
                }
            };

            // if our parent was verified before us, they're invalid
            if parent_revision < verified_at {
                break false;
            }

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == self.revision.get() {
                break true;
            }

            // cold path, deep verify dependencies
            let dependencies = memo.dependencies.lock().unwrap().clone();
            queue.push((memo, Operation::Update));
            queue.extend(dependencies.into_iter().map(|idx| {
                (
                    &self.memos[idx.0],
                    Operation::Validate {
                        parent_revision: verified_at,
                    },
                )
            }));
        };

        if valid && let Some(value) = database.value_of(idx) {
            VerificationResult::Cached { value }
        } else {
            VerificationResult::Outdated
        }
    }

    pub fn register(&mut self, database: impl Database) {
        self.databases
            .entry(database.type_id())
            .or_insert_with(|| Box::new(database));
    }
}

pub trait Database: Send + Sync + 'static {
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

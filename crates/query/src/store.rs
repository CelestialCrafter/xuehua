use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    num::NonZeroUsize,
    sync::{
        Mutex,
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
    // index_of depends on revision 0 being non-existent
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
        database.index_of(key, || {
            let idx = self.memos.push(Memo {
                verified_at: (self.revision.get() - 1).into(),
                dependencies: Mutex::default(),
            });

            KeyIndex(idx)
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
                    (memo.verified_at.load(Ordering::Acquire), parent_revision)
                }
                Operation::Update => {
                    memo.verified_at
                        .store(self.revision.get(), Ordering::Release);
                    continue;
                }
            };

            // if our parent was verified before us, they're invalid
            if parent_revision < verified_at {
                break false;
            }

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == self.revision.get() {
                continue;
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

    fn index_of(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex;
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;
    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value>;

    fn update_value(&self, idx: KeyIndex, value: Self::Value);
}

#[derive(Educe)]
#[educe(Default(new, bound(S: Default)))]
pub struct MemoryDatabase<K: Key, S: Default = RandomState> {
    lookup: Mutex<HashMap<K, KeyIndex, S>>,
    keys: Mutex<HashMap<KeyIndex, K, S>>,
    values: Mutex<HashMap<KeyIndex, K::Value, S>>,
}

impl<K: Key, S: Default + BuildHasher + Send + Sync + 'static> Database for MemoryDatabase<K, S> {
    type Key = K;
    type Value = K::Value;

    fn index_of(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
        let mut lookup = self.lookup.lock().unwrap();

        let idx = lookup.get(key).copied();
        idx.unwrap_or_else(|| {
            if let Some(idx) = lookup.get(key) {
                return *idx;
            }

            let idx = new();
            let mut keys = self.keys.lock().unwrap();
            lookup.insert(key.clone(), idx);
            keys.insert(idx, key.clone());

            idx
        })
    }

    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.keys.lock().unwrap().get(&idx).cloned()
    }

    fn value_of(&self, idx: KeyIndex) -> Option<Self::Value> {
        self.values.lock().unwrap().get(&idx).cloned()
    }

    fn update_value(&self, idx: KeyIndex, value: Self::Value) {
        self.values.lock().unwrap().insert(idx, value);
    }
}

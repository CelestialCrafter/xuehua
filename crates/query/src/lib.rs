use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    hash::{BuildHasher, Hash, RandomState},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rustc_hash::{FxHashMap, FxHashSet};

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &Context) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! input_key {
    ($ty:ty, $value:ty) => {
        impl QueryKey for $ty {
            type Value = $value;

            fn compute(self, _ctx: QueryContext) -> Self::Value {
                panic!("QueryKey::compute() should not be called on input key")
            }
        }
    };
}

pub trait Value: fmt::Debug + Clone + Send + Sync + 'static {}
impl<T: fmt::Debug + Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

#[derive(Debug)]
pub struct Memo {
    verified_at: AtomicUsize,
    dependencies: Vec<KeyIndex>,
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
pub struct MemoryDatabase<K, V, S = RandomState> {
    lookup: RwLock<HashMap<K, KeyIndex, S>>,
    keys: RwLock<HashMap<KeyIndex, K, S>>,
    values: RwLock<HashMap<KeyIndex, V, S>>,
}

impl<K: Key, V: Value, S: BuildHasher + Send + Sync + 'static> Database
    for MemoryDatabase<K, V, S>
{
    type Key = K;

    type Value = V;

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

    fn store_value(&self, idx: KeyIndex, memo: Self::Value) {
        self.values.write().unwrap().insert(idx, memo.clone());
    }
}

trait DynDatabase: Send + Sync + Any {}
impl<D: Database> DynDatabase for D {}

#[derive(Default, Educe)]
#[educe(Debug)]
struct Store {
    revision: AtomicUsize,
    index: AtomicUsize,
    #[educe(Debug(ignore))]
    databases: FxHashMap<TypeId, Box<dyn DynDatabase>>,
    memos: RwLock<FxHashMap<KeyIndex, Memo>>,
}

impl Store {
    fn database_of<K: Key>(&self) -> &K::Database {
        (self.databases[&TypeId::of::<K::Database>()].as_ref() as &dyn Any)
            .downcast_ref::<K::Database>()
            .expect("database should be of type K::Database")
    }

    fn index_of<D>(&self, database: &D, key: &D::Key) -> KeyIndex
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

    fn verify(&self, idx: KeyIndex) -> bool {
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

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == revision {
                return true;
            }

            // if dependency was verified after us, we're invalid
            if let Some(parent_revision) = parent_revision
                && parent_revision > verified_at
            {
                return false;
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

        inner(
            &self.memos.read().unwrap(),
            idx,
            self.revision.load(Ordering::Relaxed),
            None,
        )
    }
}

#[derive(Default)]
pub struct ContextBuilder {
    store: Store,
}

impl ContextBuilder {
    pub fn register_database(mut self, db: impl Database) -> Self {
        self.store.databases.insert(db.type_id(), Box::new(db));
        self
    }

    pub fn build(self) -> Context {
        Context {
            store: self.store.into(),
            dependencies: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct Context {
    store: Arc<Store>,
    dependencies: Mutex<FxHashSet<KeyIndex>>,
}

impl Context {
    pub fn new() -> Self {
        ContextBuilder::default().build()
    }

    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().insert(idx);

        if self.store.verify(idx)
            && let Some(value) = database.value_of(idx)
        {
            value
        } else {
            let ctx = Context {
                store: self.store.clone(),
                dependencies: Default::default(),
            };

            let value = key.compute(&ctx).await;
            let memo = Memo {
                verified_at: self.store.revision.load(Ordering::Relaxed).into(),
                dependencies: ctx.dependencies.into_inner().unwrap().into_iter().collect(),
            };

            database.store_value(idx, value.clone());
            self.store.memos.write().unwrap().insert(idx, memo);

            value
        }
    }

    pub fn set<K: Key>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);

        let old_revision = self.store.revision.fetch_add(1, Ordering::Relaxed);
        let revision = old_revision.wrapping_add(1);

        let memo = Memo {
            verified_at: revision.into(),
            dependencies: Default::default(),
        };

        database.store_value(idx, value);
        self.store.memos.write().unwrap().insert(idx, memo);
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;

    #[tokio::test]
    async fn expensive_query() {
        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct DifficultQuery {
            offset: u64,
        }

        impl Key for DifficultQuery {
            type Value = u64;
            type Database = MemoryDatabase<Self, Self::Value>;

            async fn compute(self, _: &Context) -> Self::Value {
                (1..=u16::MAX as u64)
                    .zip(1..=u16::MAX as u64)
                    .map(|(a, b)| if self.offset % 2 == 0 { a / b } else { a * b })
                    .reduce(|a, b| a.isqrt() * b.isqrt())
                    .unwrap()
                    + self.offset
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RootQuery {
            range: Range<u64>,
        }

        impl Key for RootQuery {
            type Value = u128;
            type Database = MemoryDatabase<Self, Self::Value>;

            async fn compute(self, ctx: &Context) -> Self::Value {
                let mut sum = 0;
                for i in self.range {
                    sum += ctx
                        .query(DifficultQuery {
                            offset: i % u16::MAX as u64,
                        })
                        .await as u128;
                }

                sum
            }
        }

        let ctx = ContextBuilder::default()
            .register_database(MemoryDatabase::<RootQuery, u128>::default())
            .register_database(MemoryDatabase::<DifficultQuery, u64>::default())
            .build();
        let result = ctx.query(RootQuery { range: 0..10000 }).await;
        println!("result: {result}");
        let result = ctx.query(RootQuery { range: 5000..15000 }).await;
        println!("result: {result}");
        let result = ctx
            .query(RootQuery {
                range: 10000..20000,
            })
            .await;
        println!("result: {result}");
        panic!()
    }
}

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

pub trait QueryKey: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: QueryValue;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &Context) -> impl Future<Output = Self::Value> + Send;
}

trait DynQueryKey: fmt::Debug + Send + Sync + Any {
    fn key_hash(&self, builder: &FxBuildHasher) -> u64;
    fn key_eq(&self, other: &dyn DynQueryKey) -> bool;
    fn key_clone(&self) -> Box<dyn DynQueryKey>;
}

impl<T: QueryKey> DynQueryKey for T {
    fn key_hash(&self, builder: &FxBuildHasher) -> u64 {
        let mut hasher = builder.build_hasher();
        self.type_id().hash(&mut hasher);
        self.hash(&mut hasher);
        hasher.finish()
    }

    fn key_eq(&self, other: &dyn DynQueryKey) -> bool {
        if let Some(other) = (other as &dyn Any).downcast_ref() {
            self == other
        } else {
            false
        }
    }

    fn key_clone(&self) -> Box<dyn DynQueryKey> {
        Box::new(self.clone())
    }
}

#[macro_export]
macro_rules! input_key {
    ($ty:ty, $value:ty) => {
        impl QueryKey for $ty {
            type Value = $value;

            fn compute(self, _ctx: QueryContext) -> Self::Value {
                panic!("QueryKey::compute() should not be called on impl InputKey")
            }
        }
    };
}

pub trait QueryValue: fmt::Debug + Clone + Send + Sync + 'static {}
impl<T: fmt::Debug + Clone + Send + Sync + 'static> QueryValue for T {}

trait DynQueryValue: Send + Sync + Any + fmt::Debug {
    fn value_clone(&self) -> Box<dyn DynQueryValue>;
}

impl<T: QueryValue> DynQueryValue for T {
    fn value_clone(&self) -> Box<dyn DynQueryValue> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

#[derive(Debug, Clone)]
pub struct Memo<V> {
    value: V,
    verified_at: usize,
    // TODO: experiment with a SmallVec instead of a Vec
    dependencies: Vec<KeyIndex>,
}

pub trait Database {
    type Key;
    type Value;

    fn index_of(&self, key: &Self::Key) -> Option<KeyIndex>;
    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key>;
    fn memo_of(&self, idx: KeyIndex) -> Option<Memo<Self::Value>>;

    fn store_key(&self, idx: KeyIndex, key: Self::Key);
    fn store_memo(&self, idx: KeyIndex, memo: Memo<Self::Value>);

    fn update_revision(&self, idx: KeyIndex, to: usize) {
        let mut memo = self.memo_of(idx).expect("memo should exist");
        memo.verified_at = to;
        self.store_memo(idx, memo);
    }
}

#[derive(Educe)]
#[educe(Default(bound(S: Default)))]
pub struct MemoryDatabase<K, V, S> {
    lookup: RwLock<HashMap<K, KeyIndex, S>>,
    keys: RwLock<HashMap<KeyIndex, K, S>>,
    memos: RwLock<HashMap<KeyIndex, Memo<V>, S>>,
}

impl<K, V, S> Database for MemoryDatabase<K, V, S> {
    type Key = K;

    type Value = V;

    fn index_of(&self, key: &Self::Key) -> Option<KeyIndex> {
        todo!()
    }

    fn key_of(&self, idx: KeyIndex) -> Option<Self::Key> {
        todo!()
    }

    fn memo_of(&self, idx: KeyIndex) -> Option<Memo<Self::Value>> {
        todo!()
    }

    fn store_key(&self, idx: KeyIndex, key: Self::Key) {
        todo!()
    }

    fn store_memo(&self, idx: KeyIndex, memo: Memo<Self::Value>) {
        todo!()
    }
}

trait DynDatabase: fmt::Debug + Any {}
impl<D: Database + fmt::Debug + 'static> DynDatabase for D {}

#[derive(Default, Educe)]
#[educe(Debug)]
struct Store {
    revision: AtomicUsize,
    index: AtomicUsize,
    databases: FxHashMap<TypeId, Box<dyn DynDatabase>>,
}

impl Store {
    fn database_of<K: QueryKey>(&self) -> &K::Database {
        (self.databases[&TypeId::of::<K>()].as_ref() as &dyn Any)
            .downcast_ref::<K::Database>()
            .expect("database should be of type K::Database")
    }

    fn index_of<D>(&self, database: &D, key: &D::Key) -> KeyIndex
    where
        D: Database,
        D::Key: QueryKey,
    {
        database.index_of(key).unwrap_or_else(|| {
            let idx = KeyIndex(self.index.fetch_add(1, Ordering::Relaxed));
            database.store_key(idx, key.clone());
            idx
        })
    }

    fn verify<D: Database>(&self, database: &D, idx: KeyIndex) -> Option<Memo<D::Value>> {
        fn inner<D: Database>(
            database: &D,
            idx: KeyIndex,
            revision: usize,
            parent_revision: Option<usize>,
        ) -> Option<Memo<D::Value>> {
            let Some(memo) = database.memo_of(idx) else {
                return None;
            };

            // hot path, if we computed the memo this revision, we know its valid
            if memo.verified_at == revision {
                return Some(memo);
            }

            // if dependency was verified after us, we're invalid
            if let Some(parent_revision) = parent_revision
                && parent_revision > memo.verified_at
            {
                return None;
            }

            // cold path, deep verify dependencies
            for dep_idx in &memo.dependencies {
                if let None = inner(database, *dep_idx, revision, Some(memo.verified_at)) {
                    return None;
                }
            }

            database.update_revision(idx, revision);
            Some(memo)
        }

        inner(database, idx, self.revision.load(Ordering::Relaxed), None)
    }
}

#[derive(Educe, Debug)]
#[educe(Default(new))]
pub struct Context {
    store: Arc<Store>,
    dependencies: Mutex<FxHashSet<KeyIndex>>,
}

impl Context {
    pub async fn query<K: QueryKey>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().insert(idx);

        match self.store.verify(database, idx) {
            Some(memo) => memo.value,
            None => {
                let ctx = Context {
                    store: self.store.clone(),
                    dependencies: Default::default(),
                };

                let value = key.compute(&ctx).await;
                let memo = Memo {
                    value: value.clone(),
                    verified_at: self.store.revision.load(Ordering::Relaxed),
                    dependencies: ctx.dependencies.into_inner().unwrap().into_iter().collect(),
                };

                database.store_memo(idx, memo);
                value
            }
        }
    }

    pub fn set<K: QueryKey>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);

        let old_revision = self.store.revision.fetch_add(1, Ordering::Relaxed);
        let revision = old_revision.wrapping_add(1);

        let memo = Memo {
            value,
            verified_at: revision,
            dependencies: Default::default(),
        };

        database.store_memo(idx, memo);
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Range, sync::Arc, time::Instant};

    use tokio::task::JoinSet;

    use super::*;

    #[tokio::test]
    async fn expensive_query() {
        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct DifficultQuery {
            offset: u64,
        }

        impl QueryKey for DifficultQuery {
            type Value = u64;

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

        impl QueryKey for RootQuery {
            type Value = u128;

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

        let mut load = 1000u64;
        let tasks = 10;
        let per_task = load / tasks;
        let ctx = Arc::new(Context::new());

        let mut joinset = JoinSet::new();
        for _ in 0..tasks {
            let range = (load - per_task)..load;
            load -= per_task;

            let ctx = ctx.clone();
            joinset.spawn(async move {
                let start = Instant::now();
                let result = ctx.query(RootQuery { range }).await;
                let duration = Instant::now().duration_since(start);
                eprintln!(
                    "finished in {}ms with result {result}",
                    duration.as_millis(),
                );
            });
        }

        joinset.join_all().await;
        panic!()
    }
}

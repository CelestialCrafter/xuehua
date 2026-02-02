use std::{
    any::Any,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use educe::Educe;
use hashbrown::HashTable;
use rustc_hash::{FxBuildHasher, FxHashSet};

pub type KeyIndex = usize;

pub trait QueryKey: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: QueryValue;

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

#[derive(Debug)]
struct Key(Box<dyn DynQueryKey>);

impl Clone for Key {
    fn clone(&self) -> Self {
        Self(self.0.key_clone())
    }
}

#[derive(Debug)]
struct Value(Box<dyn DynQueryValue>);

impl Clone for Value {
    fn clone(&self) -> Self {
        Self(self.0.value_clone())
    }
}

#[derive(Debug)]
struct Memo {
    value: Value,
    verified_at: AtomicUsize,
    // TODO: experiment with a SmallVec instead of a Vec
    dependencies: Vec<KeyIndex>,
}

#[derive(Default, Educe)]
#[educe(Debug)]
struct Store {
    revision: AtomicUsize,
    #[educe(Debug(ignore))]
    hash_builder: FxBuildHasher,
    keys: RwLock<HashTable<(KeyIndex, Key)>>,
    memos: RwLock<Vec<RwLock<Option<Arc<Memo>>>>>,
}

impl Store {
    fn index_of<K: QueryKey>(&self, key: &K) -> KeyIndex {
        let hash = key.key_hash(&self.hash_builder);
        let idx = self
            .keys
            .read()
            .unwrap()
            .find(hash, |(_, other_key)| key.key_eq(other_key.0.as_ref()))
            .map(|(idx, _)| *idx);

        idx.unwrap_or_else(|| {
            let mut memos = self.memos.write().unwrap();
            let mut keys = self.keys.write().unwrap();

            let idx = keys.len();
            memos.push(None.into());
            keys.insert_unique(hash, (idx, Key(key.key_clone())), |(_, key)| {
                key.0.key_hash(&self.hash_builder)
            });

            assert_eq!(keys.len(), memos.len());
            idx
        })
    }

    fn verify(&self, idx: KeyIndex) -> Option<Arc<Memo>> {
        fn inner(
            revision: usize,
            memos: &Vec<RwLock<Option<Arc<Memo>>>>,
            memo: &Memo,
            parent_rev: Option<usize>,
        ) -> bool {
            let verified_at = memo.verified_at.load(Ordering::Relaxed);

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == revision {
                return true;
            }

            // if dependency was verified after us, we're invalid
            if let Some(parent_revision) = parent_rev
                && parent_revision > verified_at
            {
                return false;
            }

            // cold path, deep verify dependencies
            for dep_idx in &memo.dependencies {
                let dep_memo = memos[*dep_idx].read().unwrap();
                let dep_memo = dep_memo.as_ref().expect("memo should be comptued");
                if !inner(revision, memos, dep_memo, Some(verified_at)) {
                    return false;
                }
            }

            memo.verified_at.store(revision, Ordering::Relaxed);
            true
        }

        let memos = self.memos.read().unwrap();
        let memo = memos[idx].read().unwrap();
        memo.as_ref().and_then(|memo| {
            let valid = inner(self.revision.load(Ordering::Relaxed), &memos, &memo, None);
            valid.then(|| memo.clone())
        })
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
        let idx = self.store.index_of(&key);
        self.dependencies.lock().unwrap().insert(idx);

        if let Some(memo) = self.store.verify(idx) {
            (memo.value.0.as_ref() as &dyn Any)
                .downcast_ref::<K::Value>()
                .expect("memoized value should be of type R")
                .clone()
        } else {
            let ctx = Context {
                store: self.store.clone(),
                dependencies: Default::default(),
            };

            let value = key.compute(&ctx).await;
            let memos = self.store.memos.read().unwrap();
            let mut memo = memos[idx].write().unwrap();
            *memo = Some(Arc::new(Memo {
                value: Value(value.value_clone()),
                verified_at: self.store.revision.load(Ordering::Relaxed).into(),
                dependencies: ctx.dependencies.into_inner().unwrap().into_iter().collect(),
            }));

            value
        }
    }

    pub fn set<K: QueryKey>(&mut self, key: &K, value: K::Value) {
        let value = Box::new(value);
        let old_revision = self.store.revision.fetch_add(1, Ordering::Relaxed);
        let revision = old_revision.wrapping_add(1);

        let idx = self.store.index_of(key);
        let memos = self.store.memos.read().unwrap();
        let mut memo = memos[idx].write().unwrap();
        *memo = Some(Arc::new(Memo {
            value: Value(Box::new(value)),
            verified_at: revision.into(),
            dependencies: Default::default(),
        }))
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

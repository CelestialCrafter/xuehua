pub mod store;

use std::{
    fmt,
    hash::Hash,
    sync::{Arc, atomic::Ordering},
};

use crossbeam_queue::SegQueue;
use educe::Educe;

use crate::store::{Database, Store, VerificationResult};

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &Handle) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! impl_input_key {
    ($ty:ty, $db:ty, $value:ty) => {
        impl $crate::Key for $ty {
            type Value = $value;
            type Database = $db;

            async fn compute(self, _ctx: &$crate::Handle<'_>) -> Self::Value {
                panic!("QueryKey::compute() should not be called on an input key")
            }
        }
    };
}

pub trait Value: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

pub struct Handle<'a> {
    store: &'a Arc<Store>,
    dependencies: SegQueue<KeyIndex>,
}

impl Handle<'_> {
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.push(idx);

        let memo = match self.store.verify(database, idx).await {
            VerificationResult::Outdated { memo } => memo,
            VerificationResult::Cached { value } => return value,
        };

        let store = self.store.clone();
        let handle = tokio::task::spawn(async move {
            let ctx = Handle {
                store: &store,
                dependencies: Default::default(),
            };

            (key.compute(&ctx).await, ctx.dependencies.into_iter())
        });

        let (value, dependencies) = handle.await.expect("query task should not panic");
        database.store_value(idx, value.clone());

        let mut memo_dependencies = memo.dependencies.lock().unwrap();
        memo_dependencies.clear();
        memo_dependencies.extend(dependencies);

        memo.verified_at
            .store(self.store.revision.get(), Ordering::Relaxed);

        value
    }
}

#[derive(Debug, Educe)]
#[educe(Default(new))]
pub struct Context {
    store: Arc<Store>,
}

impl Context {
    pub fn query_ctx(&self) -> Handle<'_> {
        Handle {
            store: &self.store,
            dependencies: Default::default(),
        }
    }

    fn store_mut(&mut self) -> &mut Store {
        Arc::get_mut(&mut self.store).expect("store should not have outstanding references")
    }

    pub fn set<K: Key>(&mut self, key: &K, value: K::Value) {
        let store = self.store_mut();
        store.revision = store
            .revision
            .checked_add(1)
            .expect("revision should not exceed NonZeroUsize::MAX");

        let database = store.database_of::<K>();
        let idx = store.index_of(database, key);
        database.store_value(idx, value);

        let memo = store
            .memos
            .get_mut(idx.0)
            .expect("memo should be valid for any KeyIndex");

        memo.verified_at = store.revision.get().into();
        memo.dependencies = Default::default();
    }

    pub fn register<K: Key>(mut self, database: K::Database) -> Self {
        let store = self.store_mut();
        store.register(database);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Range, sync::Mutex};

    use futures_util::{StreamExt, stream::FuturesUnordered};

    use crate::store::MemoryDatabase;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn expensive_query() {
        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct InputQuery;
        impl_input_key!(InputQuery, MemoryDatabase<Self>, Range<u64>);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct DifficultQuery {
            offset: u64,
        }

        impl Key for DifficultQuery {
            type Value = u64;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, ctx: &Handle<'_>) -> Self::Value {
                let range = (1..=u16::MAX as u64)
                    .zip(ctx.query(InputQuery).await)
                    .map(|(a, b)| a + b);
                let range = range.clone().zip(range);

                range
                    .map(|(a, b)| if self.offset % 2 == 0 { a / b } else { a * b })
                    .reduce(|a, b| a.isqrt() * b.isqrt())
                    .unwrap()
                    + self.offset
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RootQuery;

        impl Key for RootQuery {
            type Value = u128;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, ctx: &Handle<'_>) -> Self::Value {
                let mut set = FuturesUnordered::from_iter(ctx.query(InputQuery).await.map(|i| {
                    ctx.query(DifficultQuery {
                        offset: i % u16::MAX as u64,
                    })
                }));

                let mut sum = 0;
                while let Some(value) = set.next().await {
                    sum += value as u128;
                }

                sum
            }
        }

        let mut ctx = Context::new()
            .register::<RootQuery>(Default::default())
            .register::<DifficultQuery>(Default::default())
            .register::<InputQuery>(Default::default());

        ctx.set(&InputQuery, 0..1000);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 1: {result}");

        ctx.set(&InputQuery, 500..1500);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 2: {result}");

        ctx.set(&InputQuery, 0..2000);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 3: {result}");

        panic!()
    }

    #[test]
    fn expensive_normal() {
        static INPUT: Mutex<Option<Range<u64>>> = Mutex::new(None);
        fn input() -> Range<u64> {
            INPUT
                .lock()
                .unwrap()
                .as_ref()
                .expect("input should be set")
                .clone()
        }

        fn difficult(offset: u64) -> u64 {
            let range = (1..=u16::MAX as u64).zip(input()).map(|(a, b)| a + b);
            let range = range.clone().zip(range);

            range
                .map(|(a, b)| if offset % 2 == 0 { a / b } else { a * b })
                .reduce(|a, b| a.isqrt() * b.isqrt())
                .unwrap()
                + offset
        }

        fn root() -> u128 {
            input()
                .map(|i| difficult(i % u16::MAX as u64) as u128)
                .sum()
        }

        *INPUT.lock().unwrap() = Some(0..1000);
        let result = root();
        eprintln!("result 1: {result}");

        *INPUT.lock().unwrap() = Some(500..1500);
        let result = root();
        eprintln!("result 2: {result}");

        *INPUT.lock().unwrap() = Some(0..2000);
        let result = root();
        eprintln!("result 3: {result}");

        panic!()
    }
}

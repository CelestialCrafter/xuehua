pub mod store;

use std::{
    fmt,
    hash::Hash,
    sync::{Arc, Mutex},
};

use educe::Educe;

use crate::store::{Database, Memo, Store, VerificationResult};

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &QueryContext) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! impl_input_key {
    ($ty:ty, $db:ty, $value:ty) => {
        impl $crate::Key for $ty {
            type Value = $value;
            type Database = $db;

            async fn compute(self, _ctx: &$crate::QueryContext<'_>) -> Self::Value {
                panic!("QueryKey::compute() should not be called on an input key")
            }
        }
    };
}

pub trait Value: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

pub struct QueryContext<'a> {
    store: &'a Arc<Store>,
    dependencies: Mutex<Vec<KeyIndex>>,
}

impl QueryContext<'_> {
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database.as_ref(), &key);
        self.dependencies.lock().unwrap().push(idx);

        let mutex = match self.store.verify(database.as_ref(), idx) {
            VerificationResult::Cached { value } => return value,
            VerificationResult::Outdated { compute_lock } => Some(compute_lock),
            VerificationResult::New => None,
        };
        let guard = match &mutex {
            Some(mutex) => Some(mutex.lock().await),
            None => None,
        };

        let store = self.store.clone();
        let handle = tokio::task::spawn(async move {
            let ctx = QueryContext {
                store: &store,
                dependencies: Default::default(),
            };

            (
                key.compute(&ctx).await,
                ctx.dependencies.into_inner().unwrap(),
            )
        });

        let (value, dependencies) = handle.await.expect("query task should not panic");
        database.store_value(idx, value.clone());

        self.store.memos.write().unwrap().insert(
            idx,
            Memo {
                verified_at: self.store.revision.into(),
                computing: Default::default(),
                dependencies,
            },
        );

        drop(guard);
        value
    }
}

#[derive(Debug, Educe)]
#[educe(Default(new))]
pub struct Context {
    store: Arc<Store>,
}

impl Context {
    pub fn query_ctx(&self) -> QueryContext<'_> {
        QueryContext {
            store: &self.store,
            dependencies: Default::default(),
        }
    }

    pub fn set<K: Key>(&mut self, key: &K, value: K::Value) {
        let store =
            Arc::get_mut(&mut self.store).expect("store should not have outstanding references");
        store.revision += 1;

        let database = store.database_of::<K>();
        let idx = store.index_of(database.as_ref(), key);
        database.store_value(idx, value);

        let memos = store.memos.get_mut().unwrap();
        memos.insert(
            idx,
            Memo {
                verified_at: store.revision.into(),
                dependencies: Default::default(),
                computing: Default::default(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Range, sync::Mutex};

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

            async fn compute(self, ctx: &QueryContext<'_>) -> Self::Value {
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

            async fn compute(self, ctx: &QueryContext<'_>) -> Self::Value {
                let mut set = futures_util::stream::FuturesUnordered::from_iter(
                    ctx.query(InputQuery).await.map(|i| {
                        ctx.query(DifficultQuery {
                            offset: i % u16::MAX as u64,
                        })
                    }),
                );

                let mut sum = 0;
                while let Some(value) = futures_util::StreamExt::next(&mut set).await {
                    sum += value as u128;
                }

                sum
            }
        }

        let mut ctx = Context::new();

        ctx.set(&InputQuery, 0..10000);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 1: {result}");

        ctx.set(&InputQuery, 5000..15000);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 2: {result}");

        ctx.set(&InputQuery, 0..20000);
        let result = ctx.query_ctx().query(RootQuery).await;
        println!("result 3: {result}");

        panic!()
    }

    #[test]
    fn expensive_normal() {
        fn difficult(offset: u64) -> u64 {
            (1..=u16::MAX as u64)
                .zip(1..=u16::MAX as u64)
                .map(|(a, b)| if offset % 2 == 0 { a / b } else { a * b })
                .reduce(|a, b| a.isqrt() * b.isqrt())
                .unwrap()
                + offset
        }

        fn root() -> u128 {
            let mut sum = 0;
            for i in input() {
                sum += difficult(i % u16::MAX as u64) as u128;
            }

            sum
        }

        static INPUT: Mutex<Option<Range<u64>>> = Mutex::new(None);
        fn input() -> Range<u64> {
            INPUT
                .lock()
                .unwrap()
                .as_ref()
                .expect("input should be set")
                .clone()
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

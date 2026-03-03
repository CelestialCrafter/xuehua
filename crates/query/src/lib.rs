mod migration;
pub mod store;

use std::{
    any::Any,
    fmt,
    hash::Hash,
    sync::{Arc, Mutex},
};

use tokio::sync::futures::Notified;

use crate::{
    migration::{MigrationGuard, MigrationState},
    store::{Database, Memo, Store},
};

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &Context) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! impl_input_key {
    ($ty:ty, $db:ty, $value:ty) => {
        impl $crate::Key for $ty {
            type Value = $value;
            type Database = $db;

            async fn compute(self, _ctx: &$crate::Context) -> Self::Value {
                panic!("QueryKey::compute() should not be called on an input key")
            }
        }
    };
}

pub trait Value: fmt::Debug + Clone + Send + Sync + 'static {}
impl<T: fmt::Debug + Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

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
        let store = Arc::new(self.store);

        Context {
            store: store.clone(),
            dependencies: Default::default(),
            migration: MigrationState::new(store).into(),
        }
    }
}

#[derive(Debug)]
pub struct Context {
    store: Arc<Store>,
    migration: Arc<MigrationState>,
    dependencies: Mutex<Vec<KeyIndex>>,
}

impl Context {
    pub fn new() -> Self {
        ContextBuilder::default().build()
    }

    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let guard = MigrationGuard::new(&self.migration).await;

        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().push(idx);

        let value = if self.store.verify(idx, &guard)
            && let Some(value) = database.value_of(idx)
        {
            value
        } else {
            let ctx = Context {
                store: self.store.clone(),
                migration: self.migration.clone(),
                dependencies: Default::default(),
            };
            let value = key.compute(&ctx).await;
            let memo = Memo {
                verified_at: guard.revision().into(),
                dependencies: ctx.dependencies.into_inner().unwrap(),
            };

            database.store_value(idx, value.clone());
            self.store.memos.write().unwrap().insert(idx, memo);

            value
        };

        drop(guard);
        value
    }

    pub fn queue<K: Key>(&self, key: &K, value: K::Value) -> Notified<'_> {
        self.migration.queue(key, value)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::store::MemoryDatabase;

    use super::*;

    #[tokio::test]
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
        struct RootQuery;

        impl Key for RootQuery {
            type Value = u128;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, ctx: &Context) -> Self::Value {
                let mut sum = 0;
                for i in ctx.query(InputQuery).await {
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
            .register_database(MemoryDatabase::<RootQuery>::default())
            .register_database(MemoryDatabase::<DifficultQuery>::default())
            .register_database(MemoryDatabase::<InputQuery>::default())
            .build();

        ctx.queue(&InputQuery, 0..10).await;
        let result = ctx.query(RootQuery).await;
        println!("result 1: {result}");

        ctx.queue(&InputQuery, 5..15).await;
        let result = ctx.query(RootQuery).await;
        println!("result 2: {result}");

        ctx.queue(&InputQuery, 10..20).await;
        let result = ctx.query(RootQuery).await;
        println!("result 3: {result}");

        panic!()
    }
}

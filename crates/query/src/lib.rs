pub mod handle;
pub mod store;

use std::{any::TypeId, fmt, hash::Hash};

use crate::store::Database;

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, handle: &handle::Handle) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! impl_input_key {
    ($ty:ty, $db:ty, $value:ty) => {
        impl $crate::Key for $ty {
            type Value = $value;
            type Database = $db;

            async fn compute(self, _ctx: &$crate::handle::Handle<'_>) -> Self::Value {
                panic!("QueryKey::compute() should not be called on an input key")
            }
        }
    };
}

pub trait Value: fmt::Debug + PartialEq + Clone + Send + Sync + 'static {}
impl<T: fmt::Debug + PartialEq + Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize, TypeId);

#[cfg(test)]
mod tests {
    use std::{
        ops::Range,
        sync::{Arc, Mutex},
    };

    use rayon::iter::{IntoParallelIterator, ParallelIterator};
    use tokio::task::JoinSet;

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

            async fn compute(self, ctx: &handle::Handle<'_>) -> Self::Value {
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

            async fn compute(self, handle: &handle::Handle<'_>) -> Self::Value {
                let mut sum = 0;
                for i in handle.query(InputQuery).await {
                    sum += handle
                        .query(DifficultQuery {
                            offset: i % u16::MAX as u64,
                        })
                        .await as u128;
                }

                sum
            }
        }

        let root = handle::Root::new()
            .register::<RootQuery>(Default::default())
            .register::<DifficultQuery>(Default::default())
            .register::<InputQuery>(Default::default());

        let perform = async |mut root: handle::Root, range| {
            root.upcoming().update(&InputQuery, range);

            let mut joinset = JoinSet::new();
            let root = Arc::new(root);

            for _ in 0..50 {
                let root = root.clone();
                joinset.spawn(async move { root.handle().query(RootQuery).await });
            }

            let mut sum = 0;
            while let Some(value) = joinset.join_next().await {
                sum += value.expect("query should not panic");
            }

            let root = Arc::into_inner(root).expect("no more references to root should be held");
            (root, sum)
        };

        let (root, result) = perform(root, 0..1000).await;
        println!("result 1: {result:?}");

        let (root, result) = perform(root, 500..1500).await;
        println!("result 2: {result:?}");

        let (_, result) = perform(root, 0..2000).await;
        println!("result 3: {result:?}");

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
                .into_par_iter()
                .map(|i| difficult(i % u16::MAX as u64) as u128)
                .sum()
        }

        fn perform(range: Range<u64>) -> u128 {
            *INPUT.lock().unwrap() = Some(range);
            (0..50).into_par_iter().map(|_| root()).sum()
        }

        let result = perform(0..1000);
        eprintln!("result 1: {result}");

        let result = perform(500..1500);
        eprintln!("result 2: {result}");

        let result = perform(0..2000);
        eprintln!("result 3: {result}");

        panic!()
    }
}

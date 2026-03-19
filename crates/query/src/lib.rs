pub mod handle;
pub mod store;

use std::{fmt, hash::Hash};

use crate::store::Database;

pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: Value;
    type Database: Database<Key = Self, Value = Self::Value>;

    fn compute(self, ctx: &handle::Borrowed) -> impl Future<Output = Self::Value> + Send;
}

#[macro_export]
macro_rules! impl_input_key {
    ($ty:ty, $db:ty, $value:ty) => {
        impl $crate::Key for $ty {
            type Value = $value;
            type Database = $db;

            async fn compute(self, _ctx: &$crate::handle::Borrowed<'_>) -> Self::Value {
                panic!("QueryKey::compute() should not be called on an input key")
            }
        }
    };
}

pub trait Value: fmt::Debug + Clone + Send + Sync + 'static {}
impl<T: fmt::Debug + Clone + Send + Sync + 'static> Value for T {}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

#[cfg(test)]
mod tests {
    use std::{ops::Range, sync::Mutex};

    use crate::{handle::QuerySet, store::MemoryDatabase};

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

            async fn compute(self, ctx: &handle::Borrowed<'_>) -> Self::Value {
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

            async fn compute(self, handle: &handle::Borrowed<'_>) -> Self::Value {
                let mut set = QuerySet::new(handle);
                for i in handle.query(InputQuery).await {
                    set.spawn(DifficultQuery {
                        offset: i % u16::MAX as u64,
                    });
                }

                let mut sum = 0;
                while let Some(value) = set.next().await {
                    sum += value as u128;
                }

                sum
            }
        }

        let mut root = handle::Root::new()
            .register::<RootQuery>(Default::default())
            .register::<DifficultQuery>(Default::default())
            .register::<InputQuery>(Default::default());

        root.upcoming().update(&InputQuery, 0..10000);
        let result = root.borrowed().query(RootQuery).await;
        println!("result 1: {result}");

        root.upcoming().update(&InputQuery, 5000..15000);
        let result = root.borrowed().query(RootQuery).await;
        println!("result 2: {result}");

        root.upcoming().update(&InputQuery, 0..20000);
        let result = root.borrowed().query(RootQuery).await;
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

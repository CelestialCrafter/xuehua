#![warn(missing_docs)]

//! # Xuehua Query Engine
//!
//! This crate provides a tokio-based incremental computation engine
//!
//! The engine is instantiated with the [`Root`](handle::Root),
//! which can then be used to:
//! - [`Upcoming`](handle::Upcoming): Update the engine
//! - [`Handle`](handle::Handle): Query the engine

pub mod database;
pub mod handle;
mod store;

use std::{any::TypeId, fmt, hash::Hash};

use crate::database::Database;

/// The arguments of some memoized computation
pub trait Key: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    /// The resulting value of the computation
    type Value: Value;
    /// The backing storage for computed values
    type Database: Database<Key = Self, Value = Self::Value>;

    /// Returns the computed value for this key
    fn compute(self, handle: &handle::Handle) -> impl Future<Output = Self::Value> + Send;
}

#[doc(hidden)]
pub async fn _input_query<K>(_input: K, _handle: &handle::Handle<'_>) -> ! {
    panic!("QueryKey::compute() should not be called on an input key")
}

/// Helper macro to implement query keys
#[macro_export]
macro_rules! query_key {
    (internal impl $name:ident, $value:ty, $db:ty, $compute:path) => {
        impl $crate::Key for $name {
            type Value = $value;
            type Database = $db;

            async fn compute(self, handle: &$crate::handle::Handle<'_>) -> Self::Value {
                $compute(self, handle).await
            }
        }
    };

    ($visibility:vis $name:ident -> $value:ty [input, $db:ty]) => {
        query_key!($visibility $name -> $value [$crate::_input_query, $db])
    };

    ($visibility:vis $name:ident -> $value:ty [$compute:path, $db:ty]) => {
        // TODO: expand to full names
        #[derive(::std::fmt::Debug, ::std::clone::Clone, ::std::hash::Hash, ::std::cmp::PartialEq, ::std::cmp::Eq)]
        $visibility struct $name;

        query_key!(internal impl $name, $value, $db, $compute)
    };

    ($visibility:vis $name:ident($($argvalue:ty),*) -> $value:ty [$compute:path, $db:ty]) => {
        // TODO: expand to full names
        #[derive(::std::fmt::Debug, ::std::clone::Clone, ::std::hash::Hash, ::std::cmp::PartialEq, ::std::cmp::Eq)]
        $visibility struct $name ($(pub $argvalue,)*);

        query_key!(internal impl $name, $value, $db, $compute)
    };

    ($visibility:vis $name:ident($($argname:ident: $argvalue:ty),*) -> $value:ty [$compute:path, $db:ty]) => {
        // TODO: expand to full names
        #[derive(::std::fmt::Debug, ::std::clone::Clone, ::std::hash::Hash, ::std::cmp::PartialEq, ::std::cmp::Eq)]
        $visibility struct $name {
            $(pub $argname: $argvalue,)*
        }

        query_key!(internal impl $name, $value, $db, $compute)
    };
}

/// Marker trait to ensure bounds of [`Key::Value`]
pub trait Value: fmt::Debug + PartialEq + Clone + Send + Sync {}
impl<T: fmt::Debug + PartialEq + Clone + Send + Sync> Value for T {}

/// Cheaply clonable index to any given key
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize, TypeId);

#[cfg(test)]
mod tests {
    use std::{
        ops::Range,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokio::{runtime::Runtime, task::JoinSet};

    use crate::{Key, database, handle};

    #[tokio::test]
    async fn test_early_cutoff() {
        static LEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);
        static EVEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        query_key!(TextInput -> String [input, database::InMemory<Self>]);

        query_key!(LengthQuery -> usize [Self::inner, database::InMemory<Self>]);
        impl LengthQuery {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                LEN_COMPUTES.fetch_add(1, Ordering::Relaxed);
                handle.query(TextInput).await.len()
            }
        }

        query_key!(EvenQuery -> bool [Self::inner, database::InMemory<Self>]);
        impl EvenQuery {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                EVEN_COMPUTES.fetch_add(1, Ordering::Relaxed);
                handle.query(LengthQuery).await % 2 == 0
            }
        }

        let mut root = handle::Root::new()
            .register_default::<TextInput>()
            .register_default::<LengthQuery>()
            .register_default::<EvenQuery>();

        root.upcoming().update(&TextInput, "hello".to_string());
        root.handle().query(EvenQuery).await;

        assert_eq!(LEN_COMPUTES.load(Ordering::Relaxed), 1);
        assert_eq!(EVEN_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&TextInput, "world".to_string());
        root.handle().query(EvenQuery).await;

        assert_eq!(LEN_COMPUTES.load(Ordering::Relaxed), 2);
        assert_eq!(EVEN_COMPUTES.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_dynamic_dependencies() {
        static BRANCH_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, PartialEq, Clone)]
        enum FlagDirection {
            Left,
            Right,
        }

        query_key!(FlagInput -> FlagDirection [input, database::InMemory<Self>]);
        query_key!(LeftInput -> usize [input, database::InMemory<Self>]);
        query_key!(RightInput -> usize [input, database::InMemory<Self>]);

        query_key!(BranchQuery -> usize [Self::inner, database::InMemory<Self>]);
        impl BranchQuery {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                BRANCH_COMPUTES.fetch_add(1, Ordering::Relaxed);
                match handle.query(FlagInput).await {
                    FlagDirection::Left => handle.query(LeftInput).await,
                    FlagDirection::Right => handle.query(RightInput).await,
                }
            }
        }

        let mut root = handle::Root::new()
            .register_default::<FlagInput>()
            .register_default::<LeftInput>()
            .register_default::<RightInput>()
            .register_default::<BranchQuery>();

        root.upcoming().update(&FlagInput, FlagDirection::Left);
        root.upcoming().update(&LeftInput, 10);
        root.upcoming().update(&RightInput, 20);

        root.handle().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&RightInput, 99);
        root.handle().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&FlagInput, FlagDirection::Right);
        root.handle().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_compute_synchronization() {
        static SLOW_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        query_key!(SlowQuery -> () [Self::inner, database::InMemory<Self>]);
        impl SlowQuery {
            async fn inner(self, _handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                SLOW_COMPUTES.fetch_add(1, Ordering::Relaxed);
                for _ in 0..64 {
                    tokio::task::yield_now().await
                }
            }
        }

        let root: Arc<_> = handle::Root::new().register_default::<SlowQuery>().into();

        let mut joinset = JoinSet::new();
        for _ in 0..16 {
            let root_clone = root.clone();
            joinset.spawn(async move { root_clone.handle().query(SlowQuery).await });
        }

        while let Some(result) = joinset.join_next().await {
            result.expect("query should not panic")
        }

        assert_eq!(SLOW_COMPUTES.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn perftest_cpu_query() {
        query_key!(RangeInput -> Range<u128> [input, database::InMemory<Self>]);

        query_key!(Compution1Query -> u128 [Self::inner, database::InMemory<Self>]);
        impl Compution1Query {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                handle
                    .query(RangeInput)
                    .await
                    .fold(0, |acc, x| acc.isqrt().wrapping_mul(x))
            }
        }

        query_key!(Compution2Query -> [char; 16] [Self::inner, database::InMemory<Self>]);
        impl Compution2Query {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                let bytes = handle
                    .query(Compution1Query)
                    .await
                    .wrapping_pow(u32::MAX)
                    .to_le_bytes();
                bytes.map(|byte| byte as char)
            }
        }

        query_key!(Compution3Query -> [char; 16] [Self::inner, database::InMemory<Self>]);
        impl Compution3Query {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                const ROWS: usize = 8;
                const COLUMNS: usize = 16;

                let value = handle.query(Compution1Query).await;

                let mut n_matrix = [[false; COLUMNS]; ROWS];
                let mut c_matrix = [0 as char; COLUMNS];

                for row in 0..ROWS {
                    for column in 0..COLUMNS {
                        let cell = &mut n_matrix[row][column];
                        let idx = (row * ROWS) + column;
                        *cell = value >> idx != 0;
                    }

                    let cell = &mut c_matrix[row];
                    *cell = n_matrix[row].into_iter().map(|x| x as u8).sum::<u8>() as char;
                }

                c_matrix
            }
        }

        query_key!(RootQuery -> Vec<[char; 16]> [Self::inner, database::InMemory<Self>]);
        impl RootQuery {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                let mut c_matrix = vec![[0 as char; 16]; 8192];

                let range = handle.query(RangeInput).await;
                let even = range.sum::<u128>() % 2 == 0;

                for row in &mut c_matrix {
                    *row = if even {
                        handle.query(Compution3Query).await
                    } else {
                        handle.query(Compution2Query).await
                    }
                }

                c_matrix
            }
        }

        async fn perform(mut root: handle::Root, range: Range<u128>) -> handle::Root {
            root.upcoming().update(&RangeInput, range);
            root.handle().query(RootQuery).await;
            root
        }

        let mut root = handle::Root::new()
            .register_default::<RangeInput>()
            .register_default::<Compution1Query>()
            .register_default::<Compution2Query>()
            .register_default::<Compution3Query>()
            .register_default::<RootQuery>();

        let max = 2u128.pow(24);
        for _ in 0..256 {
            root = perform(root, 0..max / 2).await;
            root = perform(root, max / 2..max).await;
            root = perform(root, 0..max).await;
        }
    }

    #[test]
    fn test_query_convergence() {
        query_key!(Input1 -> u64 [input, database::InMemory<Self>]);
        query_key!(Input2 -> u64 [input, database::InMemory<Self>]);
        query_key!(Input3 -> u64 [input, database::InMemory<Self>]);
        query_key!(Input4 -> u64 [input, database::InMemory<Self>]);

        query_key!(Mutation1Query -> u64 [Self::inner, database::InMemory<Self>]);
        impl Mutation1Query {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                let lhs = handle.query(Input1).await;
                let rhs = handle.query(Input2).await;
                lhs.wrapping_add(rhs)
            }
        }

        query_key!(Mutation2Query -> u64 [Self::inner, database::InMemory<Self>]);
        impl Mutation2Query {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                let lhs = handle.query(Input3).await;
                let rhs = handle.query(Input4).await;
                lhs.wrapping_mul(rhs)
            }
        }

        query_key!(RootQuery -> u64 [Self::inner, database::InMemory<Self>]);
        impl RootQuery {
            async fn inner(self, handle: &handle::Handle<'_>) -> <Self as Key>::Value {
                let m1 = handle.query(Mutation1Query).await;
                if m1 % 2 == 0 {
                    m1.wrapping_mul(10)
                } else {
                    handle.query(Mutation2Query).await
                }
            }
        }

        async fn inner(u: &mut arbitrary::Unstructured<'_>) -> Result<(), arbitrary::Error> {
            let mut root = handle::Root::new()
                .register_default::<Input1>()
                .register_default::<Input2>()
                .register_default::<Input3>()
                .register_default::<Input4>()
                .register_default::<Mutation1Query>()
                .register_default::<Mutation2Query>()
                .register_default::<RootQuery>();

            let mut input1 = 0;
            let mut input2 = 0;
            let mut input3 = 0;
            let mut input4 = 0;

            root.upcoming().update(&Input1, input1);
            root.upcoming().update(&Input2, input2);
            root.upcoming().update(&Input3, input3);
            root.upcoming().update(&Input4, input4);

            for _ in 0..=u.arbitrary_len::<usize>()? {
                let mut upcoming = root.upcoming();

                let value = u.arbitrary::<u64>()?;
                match u.choose_index(4)? {
                    0 => {
                        input1 = value;
                        upcoming.update(&Input1, value);
                    }
                    1 => {
                        input2 = value;
                        upcoming.update(&Input2, value);
                    }
                    2 => {
                        input3 = value;
                        upcoming.update(&Input3, value);
                    }
                    3 => {
                        input4 = value;
                        upcoming.update(&Input4, value);
                    }
                    _ => unreachable!(),
                }

                let actual = root.handle().query(RootQuery).await;

                let mutation_1 = input1.wrapping_add(input2);
                let expected = if mutation_1 % 2 == 0 {
                    mutation_1.wrapping_mul(10)
                } else {
                    input3.wrapping_mul(input4)
                };

                assert_eq!(
                    actual, expected,
                    "queried computation diverged from real computation"
                );
            }

            Ok::<_, arbitrary::Error>(())
        }

        arbtest::arbtest(|u| {
            Runtime::new()
                .expect("should be able to create tokio runtime")
                .block_on(inner(u))
        });
    }
}

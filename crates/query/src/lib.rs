#![warn(missing_docs)]

//! # Xuehua Query Engine
//!
//! This crate provides a tokio-based incremental computation engine
//!
//! The engine is instantiated with the [`Engine`](engine::Engine),
//! which can then be used to:
//! - [`Upcoming`](engine::Upcoming): Update the engine
//! - [`Context`](engine::Context): Query the engine

extern crate self as xh_query;

pub mod database;
pub mod engine;
mod singleflight;
mod store;

use educe::Educe;
pub use xh_query_derive::Query;

use std::{fmt, hash::Hash};

use crate::database::{Database, EdgeDatabase};

#[doc(hidden)]
#[cfg(feature = "inventory")]
pub use inventory::submit as register_database;

#[doc(hidden)]
#[cfg(not(feature = "inventory"))]
#[macro_export]
macro_rules! register_database {
    ($($expr:tt)*) => {};
}

/// The arguments of some memoized computation
pub trait Query: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    /// The resulting value of the computation
    type Value: fmt::Debug + Send + Sync;
    /// The backing storage for computed values
    type Database: EdgeDatabase<QueryConstraint = Self> + Database<InputValue = Self::Value>;

    /// Returns the computed value for this key
    fn compute(self, qcx: &engine::Context<'_>) -> impl Future<Output = Self::Value> + Send;
}

/// Helper compute function for defining "input" keys
#[allow(clippy::unused_async)]
pub async fn input_query<K: Query>(_input: K, _qcx: &engine::Context<'_>) -> K::Value {
    panic!("Query::compute() should not be called on an input key")
}

/// Cheaply clonable index to any given key
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize);

/// The hash of a memoized value
#[derive(Educe)]
#[educe(Deref)]
pub struct Fingerprint(pub u64);

#[cfg(all(test, feature = "inventory"))]
mod tests {
    use std::{
        ops::Range,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokio::{runtime::Runtime, task::JoinSet};

    use crate::{
        Query, database,
        engine::{Context, Engine},
        input_query,
    };

    #[tokio::test]
    async fn test_early_cutoff() {
        static LEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);
        static EVEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<TextInput, String>)]
        #[compute(input_query)]
        struct TextInput;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<LengthQuery, usize>)]
        #[compute(Self::inner)]
        struct LengthQuery;
        impl LengthQuery {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                LEN_COMPUTES.fetch_add(1, Ordering::Relaxed);
                qcx.query(TextInput).await.len()
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<EvenQuery, bool>)]
        #[compute(Self::inner)]
        struct EvenQuery;
        impl EvenQuery {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                EVEN_COMPUTES.fetch_add(1, Ordering::Relaxed);
                qcx.query(LengthQuery).await % 2 == 0
            }
        }

        let mut root = Engine::new();
        root.upcoming().update(&TextInput, "hello".to_string());
        root.context().query(EvenQuery).await;

        assert_eq!(LEN_COMPUTES.load(Ordering::Relaxed), 1);
        assert_eq!(EVEN_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&TextInput, "world".to_string());
        root.context().query(EvenQuery).await;

        assert_eq!(LEN_COMPUTES.load(Ordering::Relaxed), 2);
        assert_eq!(EVEN_COMPUTES.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_dynamic_dependencies() {
        static BRANCH_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, PartialEq, Eq, Clone)]
        enum FlagDirection {
            Left,
            Right,
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<FlagInput, FlagDirection>)]
        #[compute(input_query)]
        struct FlagInput;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<LeftInput, usize>)]
        #[compute(input_query)]
        struct LeftInput;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<RightInput, usize>)]
        #[compute(input_query)]
        struct RightInput;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<BranchQuery, usize>)]
        #[compute(Self::inner)]
        struct BranchQuery;
        impl BranchQuery {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                BRANCH_COMPUTES.fetch_add(1, Ordering::Relaxed);
                match qcx.query(FlagInput).await {
                    FlagDirection::Left => qcx.query(LeftInput).await,
                    FlagDirection::Right => qcx.query(RightInput).await,
                }
            }
        }

        let mut root = Engine::new();
        root.upcoming().update(&FlagInput, FlagDirection::Left);
        root.upcoming().update(&LeftInput, 10);
        root.upcoming().update(&RightInput, 20);

        root.context().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&RightInput, 99);
        root.context().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 1);

        root.upcoming().update(&FlagInput, FlagDirection::Right);
        root.context().query(BranchQuery).await;
        assert_eq!(BRANCH_COMPUTES.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_compute_synchronization() {
        static SLOW_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<SlowQuery, ()>)]
        #[compute(Self::inner)]
        struct SlowQuery;
        impl SlowQuery {
            async fn inner(self, _qcx: &Context<'_>) -> <Self as Query>::Value {
                SLOW_COMPUTES.fetch_add(1, Ordering::Relaxed);
                for _ in 0..64 {
                    tokio::task::yield_now().await
                }
            }
        }

        let root: Arc<_> = Engine::new().into();
        let mut joinset = JoinSet::new();
        for _ in 0..16 {
            let root = root.clone();
            joinset.spawn(async move { root.context().query(SlowQuery).await });
        }

        joinset.join_all().await;
        assert_eq!(SLOW_COMPUTES.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn perftest_cpu_query() {
        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<RangeInput, Range<u128>>)]
        #[compute(input_query)]
        struct RangeInput;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Compution1Query, u128>)]
        #[compute(Self::inner)]
        struct Compution1Query {
            offset: usize,
        }
        impl Compution1Query {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let offset = u128::try_from(self.offset).unwrap_or(u128::MAX);
                let range = qcx.query(RangeInput).await;
                range
                    .map(u128::isqrt)
                    .map(|x| x as f64)
                    .map(|x| {
                        (0..x.log10() as u64)
                            .map(|x| x as f64)
                            .map(f64::asinh)
                            .map(f64::sqrt)
                            .map(|x| x.atan2(offset as f64))
                            .map(|x| x as u128)
                    })
                    .flatten()
                    .fold(0, |acc, x| acc.wrapping_mul(x + offset))
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Compution2Query, [char; 16]>)]
        #[compute(Self::inner)]
        struct Compution2Query;
        impl Compution2Query {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let bytes = qcx
                    .query(Compution1Query { offset: 0 })
                    .await
                    .wrapping_pow(u32::MAX)
                    .to_le_bytes();
                bytes.map(|byte| byte as char)
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Compution3Query, [char; 16]>)]
        #[compute(Self::inner)]
        struct Compution3Query;
        impl Compution3Query {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                const ROWS: usize = 8;
                const COLUMNS: usize = 16;

                let mut n_matrix = [[false; COLUMNS]; ROWS];
                let mut c_matrix = [0 as char; COLUMNS];

                for row in 0..ROWS {
                    for column in 0..COLUMNS {
                        let cell = &mut n_matrix[row][column];
                        let idx = (row * ROWS) + column;
                        *cell = qcx.query(Compution1Query { offset: idx }).await % 2 == 0;
                    }

                    let cell = &mut c_matrix[row];
                    *cell = n_matrix[row].into_iter().map(|x| x as u8).sum::<u8>() as char;
                }

                c_matrix
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<RootQuery, Vec<[char; 16]>>)]
        #[compute(Self::inner)]
        struct RootQuery;
        impl RootQuery {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let mut c_matrix = vec![[0 as char; 16]; 8192];

                let range = qcx.query(RangeInput).await;
                let even = range.sum::<u128>() % 2 == 0;

                for row in &mut c_matrix {
                    *row = if even {
                        qcx.query(Compution3Query).await
                    } else {
                        qcx.query(Compution2Query).await
                    }
                }

                c_matrix
            }
        }

        async fn perform(mut root: Engine, range: Range<u128>) -> Engine {
            root.upcoming().update(&RangeInput, range);
            root.context().query(RootQuery).await;
            root
        }

        let mut root = Engine::new();
        let max = 6144;
        for _ in 0..256 {
            root = perform(root, 0..max / 2).await;
            root = perform(root, max / 2..max).await;
            root = perform(root, 0..max).await;
        }
    }

    #[test]
    fn arbtest_query_convergence() {
        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Input1, u64>)]
        #[compute(input_query)]
        struct Input1;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Input2, u64>)]
        #[compute(input_query)]
        struct Input2;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Input3, u64>)]
        #[compute(input_query)]
        struct Input3;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Input4, u64>)]
        #[compute(input_query)]
        struct Input4;

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Mutation1Query, u64>)]
        #[compute(Self::inner)]
        struct Mutation1Query;
        impl Mutation1Query {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let lhs = qcx.query(Input1).await;
                let rhs = qcx.query(Input2).await;
                lhs.wrapping_add(rhs)
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<Mutation2Query, u64>)]
        #[compute(Self::inner)]
        struct Mutation2Query;
        impl Mutation2Query {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let lhs = qcx.query(Input3).await;
                let rhs = qcx.query(Input4).await;
                lhs.wrapping_mul(rhs)
            }
        }

        #[derive(Query, Debug, Clone, Hash, PartialEq, Eq)]
        #[database(database::InMemory<RootQuery, u64>)]
        #[compute(Self::inner)]
        struct RootQuery;
        impl RootQuery {
            async fn inner(self, qcx: &Context<'_>) -> <Self as Query>::Value {
                let m1 = qcx.query(Mutation1Query).await;
                if m1 % 2 == 0 {
                    m1.wrapping_mul(10)
                } else {
                    qcx.query(Mutation2Query).await
                }
            }
        }

        async fn inner(u: &mut arbitrary::Unstructured<'_>) -> Result<(), arbitrary::Error> {
            let mut root = Engine::new();

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

                let actual = root.context().query(RootQuery).await;

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

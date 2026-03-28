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
pub mod store;

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

/// Helper macro to implement "input keys"
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

/// Marker trait to ensure bounds of [`Key::Value`]
pub trait Value: fmt::Debug + PartialEq + Clone + Send + Sync {}
impl<T: fmt::Debug + PartialEq + Clone + Send + Sync> Value for T {}

/// Cheaply clonable index to any given key
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct KeyIndex(usize, TypeId);

impl KeyIndex {
    /// Constructs a new [`KeyIndex`]
    pub fn new<T: 'static>(idx: usize) -> Self {
        KeyIndex(idx, TypeId::of::<T>())
    }

    /// Retrieves the underlying index within this [`KeyIndex`]
    pub fn idx(self) -> usize {
        self.0
    }
}

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

    use crate::{Key, database::MemoryDatabase, handle};

    #[tokio::test]
    async fn test_early_cutoff() {
        static LEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);
        static EVEN_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct TextInput;
        impl_input_key!(TextInput, MemoryDatabase<Self>, String);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct LengthQuery;
        impl Key for LengthQuery {
            type Value = usize;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> Self::Value {
                LEN_COMPUTES.fetch_add(1, Ordering::Relaxed);
                handle.query(TextInput).await.len()
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct EvenQuery;
        impl Key for EvenQuery {
            type Value = bool;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> Self::Value {
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

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct FlagInput;
        impl_input_key!(FlagInput, MemoryDatabase<Self>, FlagDirection);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct LeftInput;
        impl_input_key!(LeftInput, MemoryDatabase<Self>, usize);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RightInput;
        impl_input_key!(RightInput, MemoryDatabase<Self>, usize);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct BranchQuery;
        impl Key for BranchQuery {
            type Value = usize;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> Self::Value {
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

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct SlowQuery;
        impl Key for SlowQuery {
            type Value = ();
            type Database = MemoryDatabase<Self>;

            async fn compute(self, _handle: &handle::Handle<'_>) -> Self::Value {
                SLOW_COMPUTES.fetch_add(1, Ordering::Relaxed);
                for _ in 0..50 {
                    tokio::task::yield_now().await
                }
            }
        }

        let root: Arc<_> = handle::Root::new().register_default::<SlowQuery>().into();

        let mut joinset = JoinSet::new();
        for _ in 0..20 {
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
        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RangeInput;
        impl_input_key!(RangeInput, MemoryDatabase<Self>, Range<u128>);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Compution1Query;
        impl Key for Compution1Query {
            type Value = u128;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> u128 {
                handle
                    .query(RangeInput)
                    .await
                    .fold(0, |acc, x| acc.isqrt().wrapping_mul(x))
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Compution2Query;
        impl Key for Compution2Query {
            type Value = [char; 16];
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> [char; 16] {
                let bytes = handle
                    .query(Compution1Query)
                    .await
                    .wrapping_pow(u32::MAX)
                    .to_le_bytes();
                bytes.map(|byte| byte as char)
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Compution3Query;
        impl Key for Compution3Query {
            type Value = [char; 16];
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> [char; 16] {
                const ROWS: usize = 8;
                const COLUMNS: usize = 16;

                let value = handle.query(Compution1Query).await;

                let mut n_matrix = [[false; COLUMNS]; ROWS];
                let mut c_matrix = [0 as char; COLUMNS];

                for row in 0..ROWS {
                    for column in 0..COLUMNS {
                        let cell = &mut n_matrix[dbg!(row)][dbg!(column)];
                        let idx = (row * ROWS) + column;
                        *cell = value >> idx != 0;
                    }

                    let cell = &mut c_matrix[row];
                    *cell = n_matrix[row].into_iter().map(|x| x as u8).sum::<u8>() as char;
                }

                c_matrix
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RootQuery;
        impl Key for RootQuery {
            type Value = [[char; 16]; 2048];
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> [[char; 16]; 2048] {
                let mut c_matrix = [[0 as char; 16]; 2048];

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
            std::hint::black_box(root.handle().query(RootQuery).await);
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
        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Input1;
        impl_input_key!(Input1, MemoryDatabase<Self>, u64);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Input2;
        impl_input_key!(Input2, MemoryDatabase<Self>, u64);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Input3;
        impl_input_key!(Input3, MemoryDatabase<Self>, u64);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Input4;
        impl_input_key!(Input4, MemoryDatabase<Self>, u64);

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Mutation1Query;
        impl Key for Mutation1Query {
            type Value = u64;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> u64 {
                let lhs = handle.query(Input1).await;
                let rhs = handle.query(Input2).await;
                lhs.wrapping_add(rhs)
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct Mutation2Query;
        impl Key for Mutation2Query {
            type Value = u64;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> u64 {
                let lhs = handle.query(Input3).await;
                let rhs = handle.query(Input4).await;
                lhs.wrapping_mul(rhs)
            }
        }

        #[derive(Debug, Clone, Hash, Eq, PartialEq)]
        struct RootQuery;
        impl Key for RootQuery {
            type Value = u64;
            type Database = MemoryDatabase<Self>;

            async fn compute(self, handle: &handle::Handle<'_>) -> u64 {
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

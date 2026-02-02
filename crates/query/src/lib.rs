use std::{
    any::Any,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

use educe::Educe;
use hashbrown::HashTable;
use parking_lot::RwLock;
use rustc_hash::FxBuildHasher;
use smallvec::SmallVec;

pub type KeyIndex = usize;
pub type Revision = usize;

pub trait QueryKey: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: QueryValue;

    fn compute(self, ctx: QueryContext) -> Self::Value;

    #[cfg(feature = "tokio")]
    fn compute_async(self, ctx: QueryContext) -> impl Future<Output = Self::Value> + Send {
        async {
            tokio::task::spawn_blocking(|| self.compute(ctx))
                .await
                .expect("computation panicked")
        }
    }
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

#[derive(Clone, Educe)]
#[educe(Debug, Eq(bound(F: 'static, K: PartialEq)))]
pub struct FunctionKey<F, K>(#[educe(Debug(ignore))] F, K);

impl<F: 'static, A: PartialEq> PartialEq for FunctionKey<F, A> {
    fn eq(&self, other: &Self) -> bool {
        self.0.type_id() == other.0.type_id() && self.1 == other.1
    }
}

impl<F: 'static, A: Hash> Hash for FunctionKey<F, A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.type_id().hash(state);
        self.1.hash(state);
    }
}

impl<F: Clone, K: fmt::Debug + Clone + Hash, V> QueryKey for FunctionKey<F, K>
where
    F: Fn(QueryContext, K) -> V + Send + Sync + 'static,
    K: Send + Sync + Clone + Eq + Hash + 'static,
    V: QueryValue,
{
    type Value = V;

    fn compute(self, ctx: QueryContext) -> Self::Value {
        self.0(ctx, self.1)
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
struct Memo {
    key: Box<dyn DynQueryKey>,
    value: Box<dyn DynQueryValue>,
    verified_at: Revision,
    dependencies: SmallVec<[KeyIndex; 4]>,
}

impl Clone for Memo {
    fn clone(&self) -> Self {
        Self {
            value: self.value.value_clone(),
            key: self.key.key_clone(),
            verified_at: self.verified_at.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}

#[derive(Default, Educe)]
#[educe(Debug)]
struct Store {
    #[educe(Debug(ignore))]
    hash_builder: FxBuildHasher,
    keys: RwLock<HashTable<KeyIndex>>,
    memos: boxcar::Vec<RwLock<Memo>>,
}

impl Store {
    fn index_of(&self, key: &dyn DynQueryKey) -> Option<KeyIndex> {
        self.keys
            .read()
            .find(key.key_hash(&self.hash_builder), |idx| {
                let other_key = &self.memos[*idx].read().key;
                key.key_eq(other_key.as_ref())
            })
            .copied()
    }

    fn register(&self, memo: Memo) -> KeyIndex {
        let hash = memo.key.key_hash(&self.hash_builder);
        let idx = self.memos.push(memo.into());

        self.keys.write().insert_unique(hash, idx, |idx| {
            let key = &self.memos[*idx].read().key;
            key.key_hash(&self.hash_builder)
        });

        idx
    }

    fn with_memo<R>(&self, idx: KeyIndex, func: impl FnOnce(&Memo) -> R) -> R {
        func(&*self.memos[idx].read())
    }

    fn with_memo_mut<R>(&self, idx: KeyIndex, func: impl FnOnce(&mut Memo) -> R) -> R {
        func(&mut *self.memos[idx].write())
    }
}

#[derive(Default, Clone, Debug)]
pub struct QueryContext {
    parent: Option<KeyIndex>,
    revision: Revision,
    store: Arc<Store>,
}

impl QueryContext {
    fn verify(&self, idx: KeyIndex) -> bool {
        fn inner(ctx: &QueryContext, idx: KeyIndex, parent_revision: Option<Revision>) -> bool {
            let (verified_at, dependencies) = ctx
                .store
                .with_memo(idx, |memo| (memo.verified_at, memo.dependencies.clone()));

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == ctx.revision {
                return true;
            }

            // if dependency was verified after us, we're invalid
            if let Some(parent_revision) = parent_revision
                && parent_revision > verified_at
            {
                return false;
            }

            // cold path, deep verify dependencies
            for dep_idx in dependencies {
                if !inner(ctx, dep_idx, Some(verified_at)) {
                    return false;
                }
            }

            ctx.store
                .with_memo_mut(idx, |memo| memo.verified_at = ctx.revision);
            true
        }

        inner(self, idx, None)
    }

    pub fn query<K: QueryKey>(&self, key: K) -> K::Value {
        let handle_parent = |idx| {
            if let Some(parent_idx) = self.parent {
                self.store
                    .with_memo_mut(parent_idx, |memo| memo.dependencies.push(idx))
            }
        };

        let Some(idx) = self.store.index_of(&key) else {
            let value = key.clone().compute(self.clone());
            let idx = self.store.register(Memo {
                key: Box::new(key),
                value: value.value_clone(),
                verified_at: self.revision,
                dependencies: Default::default(),
            });
            handle_parent(idx);

            return value;
        };

        handle_parent(idx);
        if self.verify(idx) {
            self.store.with_memo(idx, |memo| {
                (memo.value.as_ref() as &dyn Any)
                    .downcast_ref::<K::Value>()
                    .expect("memoized value should be of type R")
                    .clone()
            })
        } else {
            // TODO: clear dependencies before recomputation
            let value = key.compute(self.clone());
            self.store.with_memo_mut(idx, |memo| {
                memo.value = value.value_clone();
                memo.verified_at = self.revision;
            });

            value
        }
    }
}

#[derive(Default, Debug)]
pub struct Context {
    inner: QueryContext,
}

impl Deref for Context {
    type Target = QueryContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Context {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn set<K: QueryKey>(&mut self, key: &K, value: K::Value) {
        let value = Box::new(value);
        self.inner.revision = self.revision.strict_add(1);

        match self.store.index_of(key) {
            Some(idx) => self.store.with_memo_mut(idx, |memo| {
                memo.value = value;
                memo.verified_at = self.revision;
            }),
            None => {
                self.store.register(Memo {
                    key: key.key_clone(),
                    value,
                    verified_at: self.revision,
                    dependencies: Default::default(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn setup() {
        fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "[{} {}] {}",
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .level(log::LevelFilter::Trace)
            .chain(std::io::stderr())
            .apply()
            .expect("should be able to enable logger");
    }

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    struct Input;
    input_key!(Input, Vec<u64>);

    #[test]
    fn benchmark() {
        fn difficult(_: QueryContext, offset: u16) -> u64 {
            let nerf = 4;
            std::hint::black_box((1..=(u16::MAX / nerf) as u64).zip(1..=(u16::MAX / nerf) as u64))
                .map(|(a, b)| if offset % 2 == 0 { a / b } else { a * b })
                .reduce(|a, b| a.wrapping_div(b))
                .unwrap()
        }

        fn root(ctx: QueryContext, computations: u64) -> u128 {
            (0..computations)
                .map(|i| ctx.query(FunctionKey(difficult, (i % u16::MAX as u64) as u16)) as u128)
                .sum::<u128>()
        }

        let computations = 100000;
        let threads = std::thread::available_parallelism()
            .map(|v| v.get() - 4)
            .unwrap_or(8);
        let ctx = Context::new();

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let result = ctx.query(FunctionKey(root, computations));
                    let duration = Instant::now().duration_since(start);
                    println!(
                        "finished query in {}µs with result {result}",
                        duration.as_millis(),
                    );
                })
            })
            .collect();

        for thread in handles {
            thread.join().unwrap();
        }

        panic!();
    }

    #[test]
    #[should_panic]
    fn test_unset_input() {
        Context::new().query(Input);
    }
}

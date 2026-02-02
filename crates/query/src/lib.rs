use std::{
    any::Any,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    ops::Deref,
};

use educe::Educe;
use hashbrown::HashTable;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tokio::sync::{mpsc, watch};

pub type KeyIndex = usize;
pub type Revision = usize;

pub trait QueryKey: fmt::Debug + Clone + Hash + Eq + Send + Sync + 'static {
    type Value: QueryValue;

    fn compute(self, ctx: QueryContext) -> impl Future<Output = Self::Value> + Send;
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
#[educe(Debug, Eq(bound(F: 'static, A: Eq)))]
pub struct FunctionKey<F, A> {
    #[educe(Debug(ignore))]
    func: F,
    arguments: A,
}

impl<F: 'static, A: PartialEq> PartialEq for FunctionKey<F, A> {
    fn eq(&self, other: &Self) -> bool {
        self.func.type_id() == other.func.type_id() && self.arguments == other.arguments
    }
}

impl<F: 'static, A: Hash> Hash for FunctionKey<F, A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.func.type_id().hash(state);
        self.arguments.hash(state);
    }
}

impl<F: Clone, G, K: fmt::Debug + Clone + Hash, V> QueryKey for FunctionKey<F, K>
where
    F: Fn(QueryContext, K) -> G + Send + Sync + 'static,
    G: Future<Output = V> + Send,
    K: Send + Sync + Clone + Eq + Hash + 'static,
    V: QueryValue,
{
    type Value = V;

    fn compute(self, ctx: QueryContext) -> impl Future<Output = V> {
        (self.func)(ctx, self.arguments)
    }
}

#[macro_export]
macro_rules! input_key {
    ($ty:ty, $value:ty) => {
        impl QueryKey for $ty {
            type Value = $value;

            async fn compute(self, _ctx: QueryContext) -> Self::Value {
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
struct Key {
    inner: Box<dyn DynQueryKey>,
}

impl Clone for Key {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.key_clone(),
        }
    }
}

#[derive(Debug)]
struct Value {
    inner: Box<dyn DynQueryValue>,
    verified_at: Revision,
}

impl Clone for Value {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.value_clone(),
            verified_at: self.verified_at.clone(),
        }
    }
}

#[derive(Default, Clone, Educe)]
#[educe(Debug)]
struct Store {
    revision: usize,

    #[educe(Debug(ignore))]
    hash_builder: FxBuildHasher,
    keys: HashTable<(KeyIndex, Key)>,

    values: FxHashMap<KeyIndex, Value>,
    dependencies: FxHashMap<KeyIndex, Vec<KeyIndex>>,
}

impl Store {
    fn index_of(&mut self, key: &dyn DynQueryKey) -> KeyIndex {
        let hash = key.key_hash(&self.hash_builder);
        let idx = self
            .keys
            .find(hash, |(_, other_key)| key.key_eq(other_key.inner.as_ref()))
            .map(|(idx, _)| *idx);

        idx.unwrap_or_else(|| {
            let idx = self.keys.len();
            let key = Key {
                inner: key.key_clone(),
            };

            self.keys.insert_unique(hash, (idx, key), |(_, key)| {
                key.inner.key_hash(&self.hash_builder)
            });

            idx
        })
    }

    fn verify(&mut self, idx: KeyIndex) -> bool {
        fn inner(store: &mut Store, idx: KeyIndex, parent_revision: Option<Revision>) -> bool {
            let dependencies = store.dependencies.entry(idx).or_default().clone();
            let verified_at = match store.values.get(&idx) {
                Some(memo) => memo.verified_at,
                None => return false,
            };

            // hot path, if we computed the memo this revision, we know its valid
            if verified_at == store.revision {
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
                if !inner(store, dep_idx, Some(verified_at)) {
                    return false;
                }
            }

            store
                .values
                .get_mut(&idx)
                .expect("index should exist in value map")
                .verified_at = store.revision;
            true
        }

        inner(self, idx, None)
    }
}

enum Request {}

#[derive(Debug, Clone, Educe)]
#[educe(Default(new))]
pub struct Engine;

impl Engine {
    pub fn run(self) -> Context {
        let (tx, mut rx) = mpsc::channel(16);
        let (watch_tx, mut watch_rx) = watch::channel()
        tokio::task::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {}
            }
        });

        Context { inner: QueryContext { parent: None, tx, rx: watch_rx } }
    }
}

#[derive(Clone, Debug)]
pub struct QueryContext {
    parent: Option<KeyIndex>,
    tx: mpsc::Sender<Request>,
    rx: watch::Receiver<Response>
}

impl QueryContext {
    pub async fn query<K: QueryKey>(&self, key: K) -> K::Value {
        let idx = self.store.index_of(&key);
        if let Some(parent_idx) = self.parent {
            self.store
                .dependencies
                .entry(parent_idx)
                .or_default()
                .push(idx)
        }

        if self.verify(idx) {
            (self.store.values[&idx].inner.as_ref() as &dyn Any)
                .downcast_ref::<K::Value>()
                .expect("memoized value should be of type R")
                .clone()
        } else {
            self.store
                .dependencies
                .entry(idx)
                .and_modify(Vec::clear)
                .or_default();

            let value = key.compute(self).await;
            self.store.values.insert(
                idx,
                Value {
                    inner: value.value_clone(),
                    verified_at: self.revision,
                },
            );

            value
        }
    }
}

#[derive(Debug)]
pub struct Context {
    inner: QueryContext
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

        let idx = self.inner.store.index_of(key);
        self.inner.store.values.insert(
            idx,
            Value {
                inner: value,
                verified_at: self.revision,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Range, time::Instant};

    use super::*;

    #[tokio::test]
    async fn expensive_query() {
        async fn difficult(_: &mut QueryContext, offset: u64) -> u64 {
            (1..=u16::MAX as u64)
                .zip(1..=u16::MAX as u64)
                .map(|(a, b)| if offset % 2 == 0 { a / b } else { a * b })
                .reduce(|a, b| a.isqrt() * b.isqrt())
                .unwrap()
                + offset
        }

        async fn root(ctx: &mut QueryContext, range: Range<u64>) -> u128 {
            let mut sum = 0;
            for i in range {
                sum += ctx
                    .query(FunctionKey {
                        func: difficult,
                        arguments: i % u16::MAX as u64,
                    })
                    .await as u128;
            }

            sum
        }

        let ctx = Context::new();
        let start = Instant::now();
        let result = ctx
            .query(FunctionKey {
                func: root,
                arguments: 0..1000,
            })
            .await;
        let duration = Instant::now().duration_since(start);
        eprintln!(
            "finished in {}ms with result {result}",
            duration.as_millis(),
        );

        panic!()
    }
}

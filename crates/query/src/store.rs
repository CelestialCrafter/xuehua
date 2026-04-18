use std::{
    any::{Any, TypeId},
    num::NonZeroUsize,
    sync::{Mutex, atomic::AtomicUsize},
};

use educe::Educe;
use futures_util::{FutureExt, future::BoxFuture};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    Key, KeyIndex,
    database::{Difference, EdgeDatabase},
    engine::Context,
    singleflight::SingleFlight,
};

#[derive(Debug)]
pub struct Memo {
    pub dependencies: Mutex<FxHashSet<KeyIndex>>,
    pub verified_at: AtomicUsize,
    pub changed_at: AtomicUsize,
    pub flight: SingleFlight,
    pub recompute: for<'a> fn(KeyIndex, Context<'a>) -> BoxFuture<'a, Difference>,
}

#[derive(Educe, Debug)]
#[educe(Default)]
pub struct Store {
    pub databases: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
    pub memos: boxcar::Vec<Memo>,
    // revision 0 is treated as untracked in verified_at and changed_at
    #[educe(Default = NonZeroUsize::new(1).unwrap())]
    pub revision: NonZeroUsize,
}

impl Store {
    pub fn database_of<K: Key>(&self) -> &K::Database {
        let database = self
            .databases
            .get(&TypeId::of::<K::Database>())
            .expect("database should be registered");

        (database.as_ref() as &dyn Any)
            .downcast_ref()
            .expect("database should be of type K::Database")
    }

    pub fn index_of<D: EdgeDatabase>(&self, database: &D, key: &D::Key) -> KeyIndex {
        database.index(key, || {
            let idx = self.memos.push(Memo {
                verified_at: 0.into(),
                changed_at: 0.into(),
                dependencies: Mutex::default(),
                flight: SingleFlight::default(),
                recompute: Self::recompute::<D>,
            });

            KeyIndex(idx)
        })
    }

    fn recompute<D: EdgeDatabase>(idx: KeyIndex, qcx: Context<'_>) -> BoxFuture<'_, Difference> {
        let type_id = TypeId::of::<D>();
        let database = qcx.store.databases[&type_id]
            .downcast_ref::<D>()
            .expect("database should be of type D");
        let Some(key) = database.key(idx) else {
            return std::future::ready(Difference::Changed).boxed();
        };

        async move {
            let value = key.compute(&qcx).await;
            let diff = database.set_value(idx, value);

            let dependencies = qcx.dependencies.into_inner().unwrap();
            let memo = &qcx.store.memos[idx.0];
            *memo.dependencies.lock().unwrap() = dependencies;

            diff
        }
        .boxed()
    }
}

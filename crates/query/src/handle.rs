use std::sync::{Arc, Mutex, atomic::Ordering};

use educe::Educe;

use crate::{
    Key, KeyIndex,
    store::{Database, Store, VerificationResult},
};

#[derive(Debug, Educe)]
#[educe(Default(new))]
pub struct Root {
    store: Arc<Store>,
}

impl Root {
    pub fn borrowed(&self) -> Borrowed<'_> {
        Borrowed {
            store: &self.store,
            dependencies: Default::default(),
        }
    }

    fn store_mut(&mut self) -> &mut Store {
        Arc::get_mut(&mut self.store).expect("store should not have outstanding references")
    }

    pub fn upcoming(&mut self) -> Upcoming<'_> {
        let store = self.store_mut();
        store.revision = store
            .revision
            .checked_add(1)
            .expect("revision should not exceed NonZeroUsize::MAX");
        Upcoming { store }
    }

    pub fn register<K: Key>(mut self, database: K::Database) -> Self {
        let store = self.store_mut();
        store.register(database);
        self
    }
}

#[derive(Debug)]
pub struct Upcoming<'a> {
    store: &'a mut Store,
}

impl Upcoming<'_> {
    pub fn update<K: Key>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, key);
        database.store_value(idx, value);

        let memo = self
            .store
            .memos
            .get_mut(idx.0)
            .expect("memo should be valid for any KeyIndex");

        memo.verified_at = self.store.revision.get().into();
        memo.dependencies = Default::default();
    }
}

#[derive(Debug)]
pub struct Borrowed<'a> {
    store: &'a Arc<Store>,
    dependencies: Mutex<Vec<KeyIndex>>,
}

impl Borrowed<'_> {
    pub async fn query<K: Key>(&self, key: K) -> K::Value {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().push(idx);

        let memo = match self.store.verify(database, idx).await {
            VerificationResult::Outdated { memo } => memo,
            VerificationResult::Cached { value } => return value,
        };

        let sub_ctx = Borrowed {
            store: &self.store,
            dependencies: Default::default(),
        };

        let value = key.compute(&sub_ctx).await;
        database.store_value(idx, value.clone());

        *memo.dependencies.lock().unwrap() = sub_ctx.dependencies.into_inner().unwrap();

        memo.verified_at
            .store(self.store.revision.get(), Ordering::Relaxed);

        value
    }
}

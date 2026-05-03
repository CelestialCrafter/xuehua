use std::{
    any::{Any, TypeId},
    num::NonZeroUsize,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rapidhash::{RapidHashMap, RapidHashSet};

use crate::{
    Fingerprint, KeyIndex, Query, database::{DynDatabase, EdgeDatabase}, singleflight::SingleFlight
};

#[derive(Debug)]
pub struct Memo {
    pub dependencies: Mutex<RapidHashSet<KeyIndex>>,
    pub verified_at: AtomicUsize,
    pub changed_at: AtomicUsize,
    pub flight: SingleFlight,
    pub database: TypeId,
    fingerprint: AtomicU64,
}

impl Memo {
    pub fn load_fingerprint(&self, order: Ordering) -> Option<Fingerprint> {
        match self.fingerprint.load(order) {
            0 => None,
            value => Some(Fingerprint(value)),
        }
    }

    pub fn store_fingerprint_mut(&mut self, value: Option<Fingerprint>) {
        eprintln!("{:?}", value.as_ref().map(|v| v.0));
        let value = match value {
            Some(value) => *value,
            None => 0,
        };

        *self.fingerprint.get_mut() = value;
    }

    pub fn store_fingerprint(&self, value: Option<Fingerprint>, order: Ordering) {
        eprintln!("{:?}", value.as_ref().map(|v| v.0));
        let value = match value {
            Some(value) => *value,
            None => 0,
        };

        self.fingerprint.store(value, order);
    }
}

#[derive(Educe, Debug)]
#[educe(Default)]
pub struct Store {
    pub databases: RapidHashMap<TypeId, Box<dyn DynDatabase>>,
    pub memos: boxcar::Vec<Memo>,
    // revision 0 is treated as untracked in verified_at and changed_at
    #[educe(Default = NonZeroUsize::new(1).unwrap())]
    pub revision: NonZeroUsize,
}

impl Store {
    pub fn database_of<K: Query>(&self) -> &K::Database {
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
                database: database.type_id(),
                fingerprint: AtomicU64::new(0),
            });

            KeyIndex(idx)
        })
    }
}

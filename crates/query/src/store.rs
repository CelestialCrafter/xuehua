use std::{
    any::{Any, TypeId},
    cell::UnsafeCell,
    num::NonZeroUsize,
    sync::{
        Mutex,
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
};

use educe::Educe;
use rapidhash::{RapidHashMap, RapidHashSet};

use crate::{
    Fingerprint, KeyIndex, Query,
    database::{DynDatabase, EdgeDatabase},
    singleflight::SingleFlight,
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
        let value = match value {
            Some(value) => *value,
            None => 0,
        };

        *self.fingerprint.get_mut() = value;
    }

    pub fn store_fingerprint(&self, value: Option<Fingerprint>, order: Ordering) {
        let value = match value {
            Some(value) => *value,
            None => 0,
        };

        self.fingerprint.store(value, order);
    }
}

enum MemoSyncState {
    Uninitialized,
    Transitioning,
    Initialized,
}

impl MemoSyncState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Uninitialized,
            1 => Self::Transitioning,
            2 => Self::Initialized,
            _ => panic!("state is not a valid value"),
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }
}

pub struct MemoSync {
    cell: UnsafeCell<Memo>,
    state: AtomicU8,
}

impl MemoSync {
    fn fetch(&self) -> &Memo {
        let mut actual = self.state.load(Ordering::Acquire);
        let state = loop {
            match MemoSyncState::from_u8(actual) {
                MemoSyncState::Uninitialized => {
                    match self.state.compare_exchange_weak(
                        actual,
                        MemoSyncState::Transitioning.to_u8(),
                        Ordering::Release,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break MemoSyncState::Transitioning,
                        Err(next_actual) => actual = next_actual,
                    }
                }
                MemoSyncState::Transitioning => {
                    actual = self.state.load(Ordering::Acquire);
                    std::hint::spin_loop();
                    continue;
                }
                MemoSyncState::Initialized => break MemoSyncState::Initialized,
            };
        };

        match state {
            MemoSyncState::Transitioning => {
                let ptr = self.cell.get();

                // SAFETY: TODO
                unsafe { ptr.write(todo!("initialize memo")) };

                // SAFETY: TODO
                unsafe { &*ptr }
            }
            MemoSyncState::Initialized => {
                let ptr = self.cell.get();

                // SAFETY: TODO
                unsafe { &*ptr }
            }
            MemoSyncState::Uninitialized => unreachable!(),
        }
    }

    fn reset(&self) {
        let cex = || {
            // assume the state is already initialized if this is being called
            let result = self.state.compare_exchange_weak(
                MemoSyncState::Initialized.to_u8(),
                MemoSyncState::Uninitialized.to_u8(),
                Ordering::Release,
                Ordering::Relaxed,
            );

            match result {
                Ok(value) => MemoSyncState::from_u8(value),
                Err(value) => MemoSyncState::from_u8(value),
            }
        };

        let mut actual;
        loop {
            actual = cex();
            match actual {
                MemoSyncState::Uninitialized => break,
                MemoSyncState::Transitioning => {
                    std::hint::spin_loop();
                    continue;
                }
                MemoSyncState::Initialized => panic!("memo sync state should not be initialized"),
            }
        }
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

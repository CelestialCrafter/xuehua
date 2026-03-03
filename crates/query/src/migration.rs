use std::{
    any::{Any, TypeId},
    ops::AddAssign,
    sync::{Arc, Mutex, atomic::AtomicUsize},
};

use tokio::{
    sync::{Notify, RwLock, RwLockReadGuard, futures::Notified},
    task::AbortHandle,
};

use crate::{Key, KeyIndex, store::{Store, Memo}};

#[derive(Default, Debug)]
struct SharedMigrationState {
    pending: Mutex<Vec<(KeyIndex, TypeId, Box<dyn Any + Send>)>>,
    started: Notify,
    finished: Notify,
    revision: RwLock<usize>,
}

#[derive(Debug)]
pub struct MigrationState {
    handle: AbortHandle,
    shared: Arc<SharedMigrationState>,
    store: Arc<Store>,
}

impl MigrationState {
    pub fn new(store: Arc<Store>) -> Self {
        let shared = Arc::new(SharedMigrationState::default());

        let task = {
            let shared = shared.clone();
            let store = store.clone();

            tokio::task::spawn(async move {
                loop {
                    eprintln!("awaiting migration trigger");
                    shared.started.notified().await;

                    let mut revision = shared.revision.write().await;
                    revision.add_assign(1);

                    let mut memos = store.memos.write().unwrap();
                    shared
                        .pending
                        .lock()
                        .unwrap()
                        .drain(..)
                        .for_each(|(idx, ty, value)| {
                            store.databases[&ty].dyn_store_value(idx, value);
                            memos.insert(
                                idx,
                                Memo {
                                    verified_at: AtomicUsize::new(*revision),
                                    dependencies: Default::default(),
                                },
                            );
                        });

                    shared.finished.notify_waiters();
                    eprintln!("migration finished");
                }
            })
        };

        Self {
            handle: task.abort_handle(),
            shared,
            store,
        }
    }

    pub fn queue<K: Key>(&self, key: &K, value: K::Value) -> Notified<'_> {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, key);

        self.shared.pending.lock().unwrap().push((
            idx,
            TypeId::of::<K::Database>(),
            Box::new(value),
        ));

        let notified = self.shared.finished.notified();
        // if the migration thread isn't waiting for a notification yet,
        // notify_one will store the permit so we dont lose an update
        self.shared.started.notify_one();
        notified
    }
}

impl Drop for MigrationState {
    fn drop(&mut self) {
        self.handle.abort()
    }
}

#[derive(Debug)]
pub struct MigrationGuard<'a> {
    revision: RwLockReadGuard<'a, usize>,
}

impl<'a> MigrationGuard<'a> {
    pub async fn new(state: &'a MigrationState) -> Self {
        MigrationGuard {
            revision: state.shared.revision.read().await,
        }
    }

    pub fn revision(&self) -> usize {
        *self.revision
    }
}

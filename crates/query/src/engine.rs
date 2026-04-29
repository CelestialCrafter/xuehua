//! Engine handles and query execution

use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Mutex, atomic::Ordering},
};

use rustc_hash::FxHashSet;

use crate::{
    KeyIndex, Query,
    database::{Database, Difference, EdgeDatabase},
    singleflight::{FlightGuard, FlightRole},
    store::{Memo, Store},
};

/// This handle owns the engine, and loans out [`Upcoming`] and [`Context`]s to utilize it.
#[derive(Debug, Default)]
pub struct Engine {
    store: Arc<Store>,
}

impl Engine {
    /// Constructs a new [`Engine`]
    pub fn new() -> Self {
        #[cfg_attr(not(feature = "inventory"), allow(unused_mut))]
        let mut store = Store::default();

        #[cfg(feature = "inventory")]
        for erased in inventory::iter::<crate::database::ErasedDatabase> {
            let type_id = (erased.type_id_fn)();
            let database = (erased.database_fn)();
            store.databases.insert(type_id, database);
        }

        Self {
            store: Arc::new(store),
        }
    }
    /// Loan out a [`Context`] to query the engine
    pub fn context(&self) -> Context<'_> {
        Context {
            store: &self.store,
            dependencies: Mutex::default(),
        }
    }

    // FIX: refactor this function once either:
    //      - polonius stabilizes
    //      - both `Arc::is_unique` and `Arc::get_mut_unchecked` stabilize
    fn store_mut(&mut self) -> &mut Store {
        let ptr: *mut Arc<Store> = &mut self.store;
        loop {
            // SAFETY: since `ptr` comes from `&mut self`, no aliases exist
            //         we never create overlapping &mut borrows
            let arc: &mut Arc<Store> = unsafe { &mut *ptr };
            match Arc::get_mut(arc) {
                Some(store) => break store,
                None => std::hint::spin_loop(),
            }
        }
    }

    /// Loan out an [`Upcoming`] to mutate the engine
    pub fn upcoming(&mut self) -> Upcoming<'_> {
        let store = self.store_mut();
        store.revision = store
            .revision
            .checked_add(1)
            .expect("revision should not exceed NonZeroUsize::MAX");

        for database in store.databases.values_mut() {
            // TODO: evict memos from store
            let _ = database.evict_garbage();
        }

        Upcoming { store }
    }

    /// Helper function for databases that implement [`Default`]
    pub fn register_default<K>(self) -> Self
    where
        K: Query,
        K::Database: Default,
    {
        self.register(K::Database::default())
    }

    /// Registers a database into the engine
    pub fn register(mut self, database: impl EdgeDatabase) -> Self {
        self.store_mut()
            .databases
            .entry(database.type_id())
            .or_insert_with(|| Box::new(database));
        self
    }
}

/// Allows mutation of the engine's values for an upcoming revision
#[derive(Debug)]
pub struct Upcoming<'a> {
    store: &'a mut Store,
}

impl Upcoming<'_> {
    /// Update the value for any given key
    pub fn update<K: Query>(&mut self, key: &K, value: K::Value) {
        let database = self.store.database_of::<K>();
        let idx = self.store.index_of(database, key);

        database.set_value(idx, value);

        let revision = self.store.revision.get();
        let memo = self
            .store
            .memos
            .get_mut(idx.0)
            .expect("memo should be valid for any KeyIndex");

        *memo.verified_at.get_mut() = revision;
        *memo.changed_at.get_mut() = revision;
        memo.dependencies = Mutex::default();
    }
}

/// Handle to the current revision
#[derive(Debug)]
pub struct Context<'a> {
    pub(crate) store: &'a Arc<Store>,
    pub(crate) dependencies: Mutex<FxHashSet<KeyIndex>>,
}

impl Context<'_> {
    /// Queries the engine for the memoized value computed from `key`
    pub async fn query<K: Query>(&self, key: K) -> <K::Database as Database>::OutputValue<'_> {
        struct ComputeFrame<'a> {
            idx: KeyIndex,
            memo: &'a Memo,
            verified_at: usize,
            guard: FlightGuard<'a>,
        }

        enum Frame<'a> {
            Verify { idx: KeyIndex },
            ComputeRoot(ComputeFrame<'a>),
            ComputeMemo(ComputeFrame<'a>),
        }

        let revision = self.store.revision.get();
        let database = self.store.database_of::<K>();
        let root_idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().insert(root_idx);

        let post_compute = move |memo: &Memo, diff| {
            memo.verified_at.store(revision, Ordering::Release);
            if let Difference::Changed = diff {
                memo.changed_at.store(revision, Ordering::Release);
            }
        };

        let should_recompute = async |store: &Store, memo: &Memo, verified_at| {
            if verified_at == 0 {
                return true;
            }

            let dependencies = memo.dependencies.lock().unwrap().clone();
            for dep_idx in dependencies {
                let dep_memo = &store.memos[dep_idx.0];
                let _ = dep_memo.flight.takeoff().await;
                let dep_ca = dep_memo.changed_at.load(Ordering::Acquire);

                if dep_ca > verified_at {
                    return true;
                }
            }

            false
        };

        let mut queue = vec![Frame::Verify { idx: root_idx }];
        let value = loop {
            let Some(frame) = queue.pop() else {
                unreachable!("ComputeRoot should break out of the loop");
            };

            match frame {
                Frame::Verify { idx } => {
                    let memo = &self.store.memos[idx.0];
                    let verified_at = memo.verified_at.load(Ordering::Acquire);
                    if verified_at == revision {
                        if idx != root_idx {
                            continue;
                        }

                        if let Some(value) = database.value(idx) {
                            break value;
                        }

                        queue.push(Frame::ComputeRoot(ComputeFrame {
                            idx,
                            memo,
                            verified_at,
                            guard: memo.flight.pilot().await,
                        }));

                        continue;
                    }

                    let FlightRole::Pilot(guard) = memo.flight.takeoff().await else {
                        queue.push(frame);
                        continue;
                    };

                    let compute = ComputeFrame {
                        idx,
                        memo,
                        guard,
                        verified_at,
                    };
                    queue.push(if idx == root_idx {
                        Frame::ComputeRoot(compute)
                    } else {
                        Frame::ComputeMemo(compute)
                    });

                    let dependencies = memo.dependencies.lock().unwrap();
                    let dependencies = dependencies.iter().map(|&idx| Frame::Verify { idx });
                    queue.extend(dependencies);
                }
                Frame::ComputeRoot(ComputeFrame {
                    idx,
                    memo,
                    verified_at,
                    guard: _guard,
                }) => {
                    let skip_pre_post = verified_at == revision;
                    if !skip_pre_post
                        && !should_recompute(self.store, memo, verified_at).await
                        && let Some(value) = database.value(idx)
                    {
                        post_compute(memo, Difference::Unchanged);
                        break value;
                    }

                    let handle = Context {
                        store: self.store,
                        dependencies: Mutex::default(),
                    };

                    let value = key.clone().compute(&handle).await;
                    let (value, diff) = database.pass_value(idx, value);

                    if !skip_pre_post {
                        post_compute(memo, diff);

                        let dependencies = handle.dependencies.into_inner().unwrap();
                        *memo.dependencies.lock().unwrap() = dependencies;
                    }

                    break value;
                }
                Frame::ComputeMemo(ComputeFrame {
                    idx,
                    verified_at,
                    memo: _,
                    guard,
                }) => {
                    let token = guard.tokenize();
                    let store = self.store.clone();

                    tokio::task::spawn(async move {
                        let memo = &store.memos[idx.0];
                        let _guard = FlightGuard::untokenize(&memo.flight, token);

                        let diff = if should_recompute(&store, memo, verified_at).await {
                            let handle = Context {
                                store: &store,
                                dependencies: Mutex::default(),
                            };

                            let database = &store.databases[&memo.database];
                            database.recompute(idx, handle).await
                        } else {
                            Difference::Unchanged
                        };

                        post_compute(memo, diff);
                    });
                }
            }
        };

        value
    }
}

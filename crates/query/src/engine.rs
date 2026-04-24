//! Engine accessors and action execution

use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::{Arc, Mutex, atomic::Ordering},
};

use rustc_hash::{FxHashMap, FxHashSet};
use tokio::task::JoinSet;

use crate::{
    KeyIndex, Query,
    database::{Database, Difference, DynDatabase, EdgeDatabase},
    singleflight::FlightRole,
    store::{Memo, Store},
};

#[doc(hidden)]
#[linkme::distributed_slice]
pub static REGISTERED_DATABASES: [fn() -> (TypeId, Box<dyn DynDatabase>)];

/// This handle owns the engine, and loans out [`Upcoming`] and [`Context`]s to utilize it.
#[derive(Debug, Default)]
pub struct Engine {
    store: Arc<Store>,
}

impl Engine {
    /// Constructs a new [`Engine`]
    pub fn new() -> Self {
        let mut store = Store::default();
        for func in REGISTERED_DATABASES {
            let (type_id, database) = func();
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

    fn store_mut(&mut self) -> &mut Store {
        Arc::get_mut(&mut self.store).expect("store should not have outstanding references")
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

#[inline]
fn post_compute(memo: &Memo, diff: Difference, revision: usize) {
    memo.verified_at.store(revision, Ordering::Release);
    if let Difference::Changed = diff {
        memo.changed_at.store(revision, Ordering::Release);
    }
}

#[inline]
fn should_recompute(memo: &Memo, verified_at: usize, store: &Store) -> bool {
    if verified_at == 0 {
        return true;
    }

    let deps = memo.dependencies.lock().unwrap();
    for &dep_idx in deps.iter() {
        let dep_memo = &store.memos[dep_idx.0];
        if dep_memo.changed_at.load(Ordering::Acquire) > verified_at {
            return true;
        }
    }

    false
}

#[inline]
async fn evaluate_memo(idx: KeyIndex, store: Arc<Store>, revision: usize) -> KeyIndex {
    loop {
        let memo = &store.memos[idx.0];
        let verified_at = memo.verified_at.load(Ordering::Acquire);
        if verified_at == revision {
            return idx;
        }

        let _guard = match memo.flight.takeoff().await {
            FlightRole::Pilot(guard) => guard,
            FlightRole::Passenger => continue,
        };

        let diff = if should_recompute(memo, verified_at, &store) {
            let handle = Context {
                store: &store,
                dependencies: Mutex::default(),
            };

            let database = &store.databases[&memo.database];
            database.recompute(idx, handle).await
        } else {
            Difference::Unchanged
        };

        post_compute(memo, diff, revision);
        return idx;
    }
}

impl Context<'_> {
    /// Queries the engine for the memoized value computed from `key`
    pub async fn query<K: Query>(&self, key: K) -> <K::Database as Database>::OutputValue<'_> {
        let revision = self.store.revision.get();
        let database = self.store.database_of::<K>();
        let root_idx = self.store.index_of(database, &key);
        self.dependencies.lock().unwrap().insert(root_idx);

        let memo = &self.store.memos[root_idx.0];
        let mut verified_at = memo.verified_at.load(Ordering::Acquire);
        if verified_at == revision {
            if let Some(value) = database.value(root_idx) {
                return value;
            }
        }

        let mut dependents: FxHashMap<_, Vec<_>> = FxHashMap::default();
        let mut indegrees = FxHashMap::default();
        let mut queue = vec![root_idx];
        let mut visited = FxHashSet::default();
        visited.insert(root_idx);

        // discover graph
        while let Some(idx) = queue.pop() {
            let current_memo = &self.store.memos[idx.0];
            if current_memo.verified_at.load(Ordering::Acquire) == revision {
                indegrees.insert(idx, 0);
                continue;
            }

            let mut indegree = 0;
            let deps = current_memo.dependencies.lock().unwrap();
            for &dep_idx in deps.iter() {
                let dep_memo = &self.store.memos[dep_idx.0];
                if dep_memo.verified_at.load(Ordering::Acquire) != revision {
                    indegree += 1;
                    dependents.entry(dep_idx).or_default().push(idx);
                    if visited.insert(dep_idx) {
                        queue.push(dep_idx);
                    }
                }
            }

            indegrees.insert(idx, indegree);
        }

        let mut joinset = JoinSet::new();

        // schedule leaves
        for (&idx, &count) in &indegrees {
            if count == 0 && idx != root_idx {
                joinset.spawn(evaluate_memo(idx, self.store.clone(), revision));
            }
        }

        // evaluate tasks
        while let Some(result) = joinset.join_next().await {
            let idx = match result {
                Ok(idx) => idx,
                Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
                Err(err) => panic!("{err}"),
            };

            if let Some(deps) = dependents.get(&idx) {
                for &parent in deps {
                    let count = indegrees.get_mut(&parent).unwrap();
                    *count -= 1;

                    if *count == 0 && parent != root_idx {
                        joinset.spawn(evaluate_memo(parent, self.store.clone(), revision));
                    }
                }
            }
        }

        // compute root
        let mut _guard = None;
        loop {
            verified_at = memo.verified_at.load(Ordering::Acquire);
            if verified_at == revision {
                if let Some(value) = database.value(root_idx) {
                    return value;
                }
                _guard = Some(memo.flight.pilot().await);
                verified_at = memo.verified_at.load(Ordering::Acquire);
                break;
            } else {
                match memo.flight.takeoff().await {
                    FlightRole::Pilot(guard) => {
                        _guard = Some(guard);
                        break;
                    }
                    FlightRole::Passenger => {
                        let _wait = memo.flight.pilot().await;
                    }
                }
            }
        }

        let skip_pre_post = verified_at == revision;
        if !skip_pre_post {
            if !should_recompute(memo, verified_at, self.store) {
                if let Some(value) = database.value(root_idx) {
                    post_compute(memo, Difference::Unchanged, revision);
                    return value;
                }
            }
        } else if let Some(value) = database.value(root_idx) {
            return value;
        }

        let handle = Context {
            store: self.store,
            dependencies: Mutex::default(),
        };

        let value = key.compute(&handle).await;
        let (value, diff) = database.pass_value(root_idx, value);

        if !skip_pre_post {
            post_compute(memo, diff, revision);

            let dependencies = handle.dependencies.into_inner().unwrap();
            *memo.dependencies.lock().unwrap() = dependencies;
        }

        value
    }
}

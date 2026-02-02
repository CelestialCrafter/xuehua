use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    fmt::Debug,
    sync::{
        Arc, RwLock,
    },
};

use dashmap::DashMap;
use hashbrown::HashTable;
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DiGraph, NodeIndex},
};
use xh_reports::prelude::*;

#[derive(Debug, Default, IntoReport)]
#[message("could not compare query values")]
pub struct ComparisonError;

#[derive(Debug, IntoReport)]
#[message("query dependency cycle detected")]
#[context(from, to)]
pub struct CycleError {
    pub from: NodeIndex,
    pub to: NodeIndex,
}

pub type KeyId = u64;
pub type Revision = usize;

pub trait QueryValue {
    fn compare(&self, other: &Self) -> Result<bool, ComparisonError>;
}

pub trait QueryKey: Clone {
    type Value: QueryValue + 'static;

    fn compute(self, ctx: &Context) -> Self::Value;
}

pub trait ErasedQueryKey {
    fn ensure_updated(&self, ctx: &Context) -> bool;
}

impl<T: QueryKey> ErasedQueryKey for T {
    fn ensure_updated(&self, database: &Database, tracker: &Tracker) -> bool {
        let value = value
            .downcast::<T::Value>()
            .expect("query value should be Self::value");

        todo!("ensure value hasn't changed")
    }
}

// TODO: rename this struct, maybe tracker?
pub struct Tracker {
    graph: RwLock<Acyclic<DiGraph<KeyId, ()>>>,
    index: DashMap<KeyId, NodeIndex>,
}

impl Tracker {
    fn add_dependency(&self, from: KeyId, to: KeyId) -> Result<(), CycleError> {
        let mut graph = self
            .graph
            .write()
            .expect("dependency graph lock should not be poisoned");

        let from_node = *self
            .index
            .entry(from.clone())
            .or_insert_with(|| graph.add_node(from));

        let to_node = *self
            .index
            .entry(to.clone())
            .or_insert_with(|| graph.add_node(to));

        match graph.try_update_edge(from_node, to_node, ()) {
            Ok(_) => Ok(()),
            Err(_) => Err(CycleError {
                from: from_node,
                to: to_node,
            }
            .into_report()),
        }
    }

    pub fn dependencies(&self, key: &KeyId) -> Option<Vec<KeyId>> {
        let node = *self.index.get(key)?;
        let graph = self
            .graph
            .read()
            .expect("dependency graph lock should not be poisoned");

        Some(graph.neighbors(node).map(|node| graph[node]).collect())
    }
}

struct Memo {
    value: Box<dyn Any>,
    verified_at: usize
}

pub struct Context {
    parent: Option<KeyId>,
    caches: Arc<BTreeMap<TypeId, DashMap<KeyId, Memo>>>,
    key_lookup: Arc<RwLock<HashTable<KeyId>>>,
    tracker: Arc<Tracker>,
}

impl Context {
    fn ensure_key(&self, key: impl QueryKey) -> KeyId {
        self.key_lookup
    }
    pub fn query<K: QueryKey>(&self, key: K) -> K::Value {
        if let Some(parent) = self.parent {
            self.tracker
                .add_dependency(parent, id)
                .expect("queries should not contain cycles")
        };

        key.compute(&Self {
            parent: Some(key),
            tracker: self.tracker.clone(),
            caches: self.caches.clone(),
            key_lookup: self.key_lookup.clone(),
        })
    }
}

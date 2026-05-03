//! Query key, value, and memo storage

pub mod evict;
mod fallible;
mod in_memory;
pub mod persist;

use std::{any::Any, fmt, sync::atomic::Ordering};

use futures_util::{FutureExt, future::BoxFuture};

pub use fallible::Fallible;
pub use in_memory::InMemory;

use crate::{
    KeyIndex, Query,
    database::{evict::Evict, persist::Persist},
    engine::Context,
};

/// Whether a value has changed or not
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Difference {
    /// The new value is different from the old value
    Changed,
    /// The new value is equivalent to the old value
    Unchanged,
}

#[cfg(feature = "inventory")]
mod erased {
    use std::any::TypeId;

    pub use inventory::submit as register;

    use crate::database::{DynDatabase, EdgeDatabase};

    #[doc(hidden)]
    pub struct ErasedDatabase {
        pub(crate) type_id_fn: fn() -> TypeId,
        pub(crate) database_fn: fn() -> Box<dyn DynDatabase>,
    }

    inventory::collect!(ErasedDatabase);

    impl ErasedDatabase {
        pub const fn new<D: EdgeDatabase + Default>() -> Self {
            Self {
                type_id_fn: || TypeId::of::<D>(),
                database_fn: || Box::new(D::default()),
            }
        }
    }
}

#[cfg(feature = "inventory")]
pub use erased::*;

pub(crate) trait DynDatabase: Any + Send + Sync {
    fn evict_garbage(&mut self) -> Vec<KeyIndex>;
    fn recompute<'a>(&'a self, idx: KeyIndex, qcx: Context<'a>) -> BoxFuture<'a, Difference>;
}

impl fmt::Debug for dyn DynDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self as &dyn Any).fmt(f)
    }
}

/// Marker trait for databases that take in their key's output
pub trait EdgeDatabase: Database<Key = Self::QueryConstraint> {
    #[doc(hidden)]
    type QueryConstraint: Query<Value = Self::InputValue>;
}

impl<T: Database> EdgeDatabase for T
where
    Self::Key: Query<Value = Self::InputValue>,
{
    type QueryConstraint = Self::Key;
}

impl<D: EdgeDatabase> DynDatabase for D {
    fn evict_garbage(&mut self) -> Vec<KeyIndex> {
        self.eviction().evict_garbage()
    }

    fn recompute<'a>(&'a self, idx: KeyIndex, qcx: Context<'a>) -> BoxFuture<'a, Difference> {
        let Some(key) = self.key(idx) else {
            return std::future::ready(Difference::Changed).boxed();
        };

        async move {
            let value = key.compute(&qcx).await;
            let (value, diff) = self.pass_value(idx, value);
            let fingerprint = self.persistence().fingerprint(&value);

            let dependencies = qcx.dependencies.into_inner().unwrap();
            let memo = &qcx.store.memos[idx.0];
            memo.store_fingerprint(fingerprint, Ordering::Release);
            *memo.dependencies.lock().unwrap() = dependencies;

            diff
        }
        .boxed()
    }
}

/// Trait for storage of computed values.
///
/// Implementors must ensure that the database operates logically
/// (eg. after `set_value`, `value_of` should return Some).
pub trait Database: Send + Sync + 'static {
    /// Keys the database can handle.
    type Key;

    /// Values the database can take as an input.
    type InputValue;

    /// Values the database returns as an output.
    type OutputValue<'a>;

    /// Eviction strategy the database can use.
    type EvictionExtension<'a>: Evict;
    /// Persistence strategy the database can use.
    type PersistExtension<'a>: Persist<Value<'a> = Self::OutputValue<'a>>;

    /// Returns the index or identifier of a given key.
    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex;

    /// Returns the key at a given index.
    fn key(&self, idx: KeyIndex) -> Option<Self::Key>;

    /// Returns the value at a given index.
    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>>;

    /// Updates the value at a given index, and returns a corresponding instance of [`Self::OutputValue`].
    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference);

    /// Returns the persistence database extension.
    fn persistence(&self) -> &Self::PersistExtension<'_>;

    /// Returns the eviction database extension.
    fn eviction(&mut self) -> &mut Self::EvictionExtension<'_>;
}

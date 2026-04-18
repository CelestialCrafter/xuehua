//! Query key, value, and memo storage

mod in_memory;
pub use in_memory::InMemory;

use crate::{Key, KeyIndex};

/// Whether a value has changed or not
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Difference {
    /// The new value is different from the old value
    Changed,
    /// The new value is equivalent to the old value
    Unchanged,
}

/// Marker trait for databases that take in their key's output
pub trait EdgeDatabase: Database<Key = Self::Constraint> {
    #[doc(hidden)]
    type Constraint: Key<Value = Self::InputValue>;
}

impl<T: Database> EdgeDatabase for T
where
    Self::Key: Key<Value = Self::InputValue>,
{
    type Constraint = Self::Key;
}

/// Trait for storage of computed values
///
/// Implementors must ensure that the database operates logically
/// (eg. after `set_value`, `value_of` should return Some)
pub trait Database: Send + Sync + 'static {
    /// Keys the database can handle
    type Key;

    /// Values the database can take as an input
    type InputValue;

    /// Values the database returns as an output
    type OutputValue<'a>;

    /// Returns the index or identifier of a given key
    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex;

    /// Returns the key at a given index
    fn key(&self, idx: KeyIndex) -> Option<Self::Key>;

    /// Returns the value at a given index
    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>>;

    /// Updates the value at a given index
    fn set_value(&self, idx: KeyIndex, value: Self::InputValue) -> Difference;

    /// Same as [`set_value`], except returns the corresponding instance of [`Self::OutputValue`]
    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        let diff = self.set_value(idx, value);
        let value = self.value(idx).expect("value should exist");

        (value, diff)
    }
}

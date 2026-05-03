use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{
    Fingerprint, KeyIndex,
    database::{Database, Difference, persist::Persist},
};

/// Database adapter implementing serialization and deserialization of values via [`postcard`].
pub struct Postcard<D> {
    inner: D,
}

impl<D> Persist for Postcard<D>
where
    D: Database,
    for<'a> D::OutputValue<'a>: Serialize + Deserialize<'a>,
{
    type Value<'a>
        = D::OutputValue<'a>
    where
        Self: 'a;

    fn fingerprint<'a>(&'a self, value: &Self::Value<'a>) -> Option<Fingerprint> {
        self.inner.persistence().fingerprint(value)
    }

    fn serialize<'a>(&'a self, value: &Self::Value<'a>) -> Option<Bytes> {
        postcard::to_allocvec(value).map(Bytes::from_owner).ok()
    }

    fn deserialize<'a>(&'a self, data: &'a Bytes) -> Option<Self::Value<'a>> {
        postcard::from_bytes(&data).ok()
    }
}

impl<D> Database for Postcard<D>
where
    D: Database,
    for<'a> D::OutputValue<'a>: Serialize + Deserialize<'a>,
{
    type Key = D::Key;
    type InputValue = D::InputValue;
    type OutputValue<'a> = D::OutputValue<'a>;
    type EvictionExtension<'a> = D::EvictionExtension<'a>;
    type PersistExtension<'a> = Self;

    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
        self.inner.index(key, new)
    }

    fn key(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.inner.key(idx)
    }

    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>> {
        self.inner.value(idx)
    }

    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        self.inner.pass_value(idx, value)
    }

    fn persistence(&self) -> &Self::PersistExtension<'_> {
        self
    }

    fn eviction(&mut self) -> &mut Self::EvictionExtension<'_> {
        self.inner.eviction()
    }
}

use std::hash::{BuildHasher, Hash, Hasher};

use bytes::Bytes;

use crate::{
    Fingerprint, KeyIndex,
    database::{
        Database, Difference,
        persist::{COMPILATION_SALT, Persist},
    },
};

/// Database adapter allowing persistence fingerprinting of values.
#[derive(Default)]
pub struct Fingerprinting<S, D> {
    build_hasher: S,
    inner: D,
}

impl<S, D> Persist for Fingerprinting<S, D>
where
    S: BuildHasher + 'static,
    D: Database,
    for<'a> D::OutputValue<'a>: Hash,
    D::OutputValue<'static>: 'static,
{
    type Value<'a>
        = D::OutputValue<'a>
    where
        Self: 'a;

    // ideally we'd hash the TypeId aswell, but we cant..
    fn fingerprint<'a>(&'a self, value: &Self::Value<'a>) -> Option<Fingerprint> {
        let mut hasher = self.build_hasher.build_hasher();
        value.hash(&mut hasher);
        COMPILATION_SALT.hash(&mut hasher);

        Some(Fingerprint(hasher.finish()))
    }

    fn serialize<'a>(&'a self, value: &Self::Value<'a>) -> Option<Bytes> {
        self.inner.persistence().serialize(value)
    }

    fn deserialize(&self, data: Bytes) -> Option<Self::Value<'_>> {
        self.inner.persistence().deserialize(data)
    }
}

impl<S, D> Database for Fingerprinting<S, D>
where
    S: BuildHasher + Sync + Send + 'static,
    D: Database,
    for<'a> D::OutputValue<'a>: Hash,
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

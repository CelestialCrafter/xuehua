//! Value persistence

use std::marker::PhantomData;

use bytes::Bytes;
use educe::Educe;

use crate::Fingerprint;

/// Database extension to support persisting values.
pub trait Persist {
    /// Value type to persist.
    type Value<'a> where Self: 'a;

    /// Computes the fingerprint for the given value.
    fn fingerprint<'a>(&'a self, value: &Self::Value<'a>) -> Option<Fingerprint>;

    /// Serializes the given value into bytes.
    fn serialize<'a>(&'a self, value: &Self::Value<'a>) -> Option<Bytes>;

    /// Deserializes bytes into the given value.
    fn deserialize(&self, data: Bytes) -> Option<Self::Value<'_>>;
}

/// No-Op persistence extension for when a database does not support persistence.
#[derive(Educe)]
#[educe(
    Default(bound()),
    Clone(bound()),
    Debug(bound()),
    Copy
)]
pub struct NoOp<V>(PhantomData<V>);
impl<V> Persist for NoOp<V> {
    type Value<'a> = V where V: 'a;

    fn fingerprint<'a>(&'a self, _value: &Self::Value<'a>) -> Option<Fingerprint> {
        None
    }

    fn serialize<'a>(&'a self, _value: &Self::Value<'a>) -> Option<Bytes> {
        None
    }

    fn deserialize(&self, _data: Bytes) -> Option<Self::Value<'_>> {
        None
    }
}

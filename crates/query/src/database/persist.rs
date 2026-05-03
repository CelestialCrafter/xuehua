//! Value persistence

mod fingerprinting;
pub use fingerprinting::Fingerprinting;

#[cfg(feature = "postcard")]
mod postcard;
#[cfg(feature = "postcard")]
pub use postcard::Postcard;

use std::marker::PhantomData;

use bytes::Bytes;
use educe::Educe;

use crate::Fingerprint;

/// Salt to append to fingerprints, ensuring they change when needed.
///
/// Ensuring that fingerprints change across compilations helps invalidate values whenever properties such as field ordering or implementation details change.
///
/// Determined by generating a random number at compile-time, and inserting it into the `COMPILATION_SALT` environment variable.
/// Evil Syntax Sugar is also a common name for this.
pub const COMPILATION_SALT: &str = env!("COMPILATION_SALT");

/// Database extension to support persisting values.
pub trait Persist {
    /// Value type to persist.
    type Value<'a>
    where
        Self: 'a;

    /// Computes the fingerprint for the given value.
    fn fingerprint<'a>(&'a self, value: &Self::Value<'a>) -> Option<Fingerprint>;

    /// Serializes the given value into bytes.
    fn serialize<'a>(&'a self, value: &Self::Value<'a>) -> Option<Bytes>;

    /// Deserializes bytes into the given value.
    fn deserialize<'a>(&'a self, data: &'a Bytes) -> Option<Self::Value<'a>>;
}

/// No-Op persistence extension for when a database does not support persistence.
#[derive(Educe)]
#[educe(Default(bound(), new), Debug(bound()))]
pub struct NoOp<V>(PhantomData<V>);
impl<V> Persist for NoOp<V> {
    type Value<'a>
        = V
    where
        V: 'a;

    fn fingerprint<'a>(&'a self, _value: &Self::Value<'a>) -> Option<Fingerprint> {
        None
    }

    fn serialize<'a>(&'a self, _value: &Self::Value<'a>) -> Option<Bytes> {
        None
    }

    fn deserialize<'a>(&'a self, _data: &'a Bytes) -> Option<Self::Value<'a>> {
        None
    }
}

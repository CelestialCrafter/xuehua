use std::marker::PhantomData;

use bytes::Bytes;
use educe::Educe;

use crate::{
    Fingerprint, KeyIndex,
    database::{Database, persist::Persist},
};

use super::Difference;

/// Database adapter that forwards only [`Result`]s of variant `Ok(T)`, ignoring all `Err(E)` variants
#[derive(Educe)]
#[educe(Default(bound(D: Default)))]
pub struct Fallible<E, D> {
    inner: D,
    _marker: PhantomData<E>,
}

impl<E, D: Database> Persist for Fallible<E, D> {
    type Value<'a>
        = Result<<D::PersistExtension<'a> as Persist>::Value<'a>, E>
    where
        Self: 'a;

    fn fingerprint<'a>(&'a self, value: &Self::Value<'a>) -> Option<Fingerprint> {
        let persistence = self.inner.persistence();
        match value {
            Ok(value) => persistence.fingerprint(value),
            Err(_) => None,
        }
    }

    fn serialize<'a>(&'a self, value: &Self::Value<'a>) -> Option<Bytes> {
        match value {
            Ok(value) => self.inner.persistence().serialize(value),
            Err(_) => None,
        }
    }

    fn deserialize<'a>(&'a self, data: &'a Bytes) -> Option<Self::Value<'a>> {
        self.inner.persistence().deserialize(data).map(Ok)
    }
}

impl<E, D> Database for Fallible<E, D>
where
    E: Send + Sync + 'static,
    D: Database,
{
    type Key = D::Key;
    type InputValue = Result<D::InputValue, E>;
    type OutputValue<'a> = Result<D::OutputValue<'a>, E>;
    type PersistExtension<'a> = Self;
    type EvictionExtension<'a> = D::EvictionExtension<'a>;

    fn index(&self, key: &Self::Key, new: impl FnOnce() -> KeyIndex) -> KeyIndex {
        self.inner.index(key, new)
    }

    fn key(&self, idx: KeyIndex) -> Option<Self::Key> {
        self.inner.key(idx)
    }

    fn value(&self, idx: KeyIndex) -> Option<Self::OutputValue<'_>> {
        self.inner.value(idx).map(Ok)
    }

    fn pass_value(
        &self,
        idx: KeyIndex,
        value: Self::InputValue,
    ) -> (Self::OutputValue<'_>, Difference) {
        match value {
            Ok(value) => {
                let (value, diff) = self.inner.pass_value(idx, value);
                (Ok(value), diff)
            }
            Err(err) => (Err(err), Difference::Changed),
        }
    }

    fn persistence(&self) -> &Self::PersistExtension<'_> {
        self
    }

    fn eviction(&mut self) -> &mut Self::EvictionExtension<'_> {
        self.inner.eviction()
    }
}

#[cfg(all(test, feature = "inventory"))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        Query,
        database::{Fallible, self},
        engine::{Context, Engine},
    };

    #[tokio::test]
    async fn test_fallible() {
        static FALLIBLE_COMPUTES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Hash, Debug, Eq, Clone)]
        struct Hidden {
            value: usize,
        }

        impl PartialEq for Hidden {
            fn eq(&self, _other: &Self) -> bool {
                true
            }
        }

        #[derive(Query, Debug, Clone, Copy, Hash, PartialEq, Eq)]
        #[database(Fallible<Hidden, database::Default<FallibleQuery, Hidden>>)]
        #[compute(Self::inner)]
        struct FallibleQuery {
            ok: bool,
        }

        impl FallibleQuery {
            async fn inner(self, _qcx: &Context<'_>) -> <Self as Query>::Value {
                let hidden = Hidden {
                    value: FALLIBLE_COMPUTES.fetch_add(1, Ordering::Relaxed),
                };

                if self.ok { Ok(hidden) } else { Err(hidden) }
            }
        }

        let root = Engine::new();
        let extract = |value| match value {
            Ok(Hidden { value }) => value,
            Err(Hidden { value }) => value,
        };

        let key = FallibleQuery { ok: true };
        let a = root.context().query(key).await;
        let b = root.context().query(key).await;
        assert_eq!(extract(a), extract(b));

        let key = FallibleQuery { ok: false };
        let a = root.context().query(key).await;
        let b = root.context().query(key).await;
        assert_ne!(extract(a), extract(b));
    }
}

use std::{fmt, marker::PhantomData, str::FromStr};

use serde::{
    Deserializer, Serializer,
    de::{Error as DeError, Visitor},
};

// stolen from https://users.rust-lang.org/t/serde-fromstr-on-a-field/99457/5
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: fmt::Display,
    D: Deserializer<'de>,
{
    struct Helper<S>(PhantomData<S>);
    impl<'de, S> Visitor<'de> for Helper<S>
    where
        S: FromStr,
        S::Err: fmt::Display,
    {
        type Value = S;

        fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "a string")
        }

        fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
            value.parse::<Self::Value>().map_err(E::custom)
        }
    }

    deserializer.deserialize_str(Helper(PhantomData))
}

pub fn serialize<T: fmt::Display, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

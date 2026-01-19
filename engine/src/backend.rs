use std::{
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
};

use serde::{Serialize, de::DeserializeOwned};
use xh_reports::{IntoReport, Result};

pub trait Backend {
    type Error: IntoReport;
    type Value: Debug + Clone + PartialEq + Send + Sync;

    fn serialize<T: Serialize>(&self, value: &T) -> Result<Self::Value, Self::Error>;
    fn deserialize<T: DeserializeOwned>(&self, value: Self::Value) -> Result<T, Self::Error>;

    fn hash(&self, hasher: &mut blake3::Hasher, value: &Self::Value) -> Result<(), Self::Error> {
        let root: serde_json::Value = self.deserialize(value.clone())?;
        let mut std_hasher = DefaultHasher::new();
        root.hash(&mut std_hasher);
        hasher.update(&std_hasher.finish().to_le_bytes());

        Ok(())
    }
}

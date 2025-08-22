use serde::{Serialize, de::DeserializeOwned};

pub type Value = serde_json::Value;
pub type Error = serde_json::Error;

pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    serde_json::from_value(value)
}

pub fn to_value<T: Serialize>(value: T) -> Result<Value, Error> {
    serde_json::to_value(value)
}

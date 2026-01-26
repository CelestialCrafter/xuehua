use std::{fmt::Debug, path::Path};

use xh_reports::{IntoReport, Result};

use crate::{Value, planner::Planner};

pub trait Backend {
    type Error: IntoReport;
    type Value: Debug + Clone + PartialEq + Send + Sync;

    fn serialize(&self, value: &Value) -> Result<Self::Value, Self::Error>;
    fn deserialize(&self, value: Self::Value) -> Result<Value, Self::Error>;

    fn plan(&self, planner: &mut Planner, project: &Path) -> Result<(), Self::Error>;
}

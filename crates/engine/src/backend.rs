use std::{fmt::Debug, path::Path};

use xh_reports::{IntoReport, Result};

use crate::planner::Planner;

pub trait Backend {
    type Error: IntoReport;
    type Value: Debug + Clone + PartialEq + Send + Sync;


    fn plan(&self, planner: &mut Planner, project: &Path) -> Result<(), Self::Error>;
}

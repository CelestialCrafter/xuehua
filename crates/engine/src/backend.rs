use std::{fmt::Debug, path::Path};

use xh_reports::{IntoReport, Result};

use crate::planner::{Planner, Unfrozen};

pub trait Backend {
    type Error: IntoReport;
    type Value: Debug + Clone + PartialEq + Send + Sync;

    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Self::Error>;
}

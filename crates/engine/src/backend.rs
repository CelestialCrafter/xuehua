use std::path::Path;

use xh_reports::{IntoReport, Result};

use crate::{
    name::BackendName,
    planner::{Planner, Unfrozen},
};

pub trait Backend {
    type Error: IntoReport;
    type Value: std::fmt::Debug + Clone;

    fn name() -> &'static BackendName;

    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Self::Error>;
}

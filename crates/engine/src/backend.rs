use std::path::Path;

use xh_reports::prelude::*;

use crate::{
    name::BackendName,
    planner::{Planner, Unfrozen},
};

#[derive(Default, Debug, IntoReport)]
#[message("could not run backend")]
pub struct Error;

pub trait Backend {
    type Value: std::fmt::Debug + Clone;

    fn name() -> &'static BackendName;
    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Error>;
}

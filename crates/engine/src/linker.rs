use xh_archive::Event;
use xh_reports::prelude::*;

use crate::store::ArtifactId;

#[derive(Debug, Default, Clone, IntoReport)]
#[message("could not (un)link artifact")]
pub struct Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Linked,
    Unlinked,
}

pub trait Linker {
    fn link(
        &mut self,
        artifact: ArtifactId,
        content: impl Iterator<Item = Event>,
    ) -> Result<(), Error>;
    fn unlink(&mut self, artifact: ArtifactId) -> Result<(), Error>;

    fn state(&self, artifact: ArtifactId) -> Result<LinkState, Error>;
}

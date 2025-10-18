use std::path::PathBuf;

use rustix::fs::RenameFlags;
use tempfile::TempDir;
use xh_archive::{Event, unpacking::Unpacker};
use xh_engine::{
    linker::{Error, LinkState, Linker},
    store::ArtifactId,
};
use xh_reports::prelude::*;

pub struct FileLinker {
    root: PathBuf,
}

impl FileLinker {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Linker for FileLinker {
    fn link(
        &mut self,
        artifact: ArtifactId,
        content: impl Iterator<Item = Event>,
    ) -> Result<(), Error> {
        let temp = TempDir::new().wrap()?;

        let mut unpacker = Unpacker::new(temp.path());
        unsafe { unpacker.unpack_mmap_iter(content) }.wrap()?;

        rustix::fs::renameat_with(
            old_dirfd,
            old_path,
            new_dirfd,
            ,
            RenameFlags::NOREPLACE.union(RenameFlags::EXCHANGE),
        )
        .wrap()?;

        Ok(())
    }

    fn unlink(&mut self, artifact: ArtifactId) -> Result<(), Error> {
        todo!("artifact unlinking")
    }

    fn state(&self, artifact: ArtifactId) -> Result<LinkState, Error> {
        todo!("artifact state query")
    }
}

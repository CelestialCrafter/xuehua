//! Unpacking of [`Event`]s into the filesystem

use std::{
    borrow::Borrow,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Component, Path},
};

use bytes::Bytes;
use thiserror::Error;

use crate::{Object, ObjectContent, PathBytes, utils::debug};

/// Error type for unpacking
#[derive(Error, Debug)]
pub enum Error {
    /// An invalid path was in the index
    #[error("invalid path: {0:?}")]
    InvalidPath(PathBytes),
    #[allow(missing_docs)]
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

// TODO: impl overwrite option
// TODO: make unpacker stateless
/// Packer for archive events.
///
/// The unpacker consumes [`Event`]s and unpacks them to the filesystem.
pub struct Unpacker<'a> {
    root: &'a Path,
}

type WriteFileFn = fn(&Path, &Bytes) -> Result<(), Error>;

fn verify_path(path: &PathBytes) -> Result<&PathBytes, Error> {
    if path
        .as_ref()
        .components()
        .all(|c| matches!(c, Component::Normal(..)))
    {
        Ok(path)
    } else {
        Err(Error::InvalidPath(path.clone()))
    }
}

impl<'a> Unpacker<'a> {
    /// Constructs a new unpacker.
    #[inline]
    pub fn new(root: &'a Path) -> Self {
        Self { root }
    }

    /// Unpacks an iterator of [`Event`]s onto the filesystem.
    #[inline]
    pub fn unpack(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Object>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow(), write_file_default))
    }

    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Object>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow(), write_file_mmap))
    }

    fn process(&mut self, object: &Object, write_file: WriteFileFn) -> Result<(), Error> {
        debug!("unpacking object: {object:?}");

        self.process_object(object, write_file)
    }

    fn process_object(&self, object: &Object, write_file: WriteFileFn) -> Result<(), Error> {
        let location = self.root.join(verify_path(&object.location)?);
        debug!("unpacking to {}", location.display());

        let set_permissions =
            || fs::set_permissions(&location, fs::Permissions::from_mode(object.permissions));

        match &object.content {
            ObjectContent::File { data } => {
                write_file(&location, &data)?;
                set_permissions()?;
            }
            ObjectContent::Symlink { target } => symlink(target, &location)?,
            ObjectContent::Directory => {
                fs::create_dir(&location)?;
                set_permissions()?;
            }
        };

        Ok(())
    }
}

#[inline]
fn write_file_default(path: &Path, contents: &Bytes) -> Result<(), Error> {
    fs::write(path, contents).map_err(Into::into)
}

#[cfg(feature = "mmap")]
#[inline]
fn write_file_mmap(path: &Path, contents: &Bytes) -> Result<(), Error> {
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    file.set_len(contents.len() as u64)?;

    let mut map = unsafe {
        memmap2::MmapOptions::new()
            .len(contents.len())
            .map_mut(&file)
    }?;
    map.advise(memmap2::Advice::Sequential)?;

    map.copy_from_slice(&contents);

    Ok(())
}

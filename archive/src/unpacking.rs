//! Unpacking of [`Event`]s into the filesystem

use std::{
    borrow::Borrow,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Component, Path},
};

use bytes::Bytes;
use thiserror::Error;

use crate::{Event, Object, ObjectContent, PathBytes, utils::debug};

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
    pub fn unpack_iter(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.unpack(event))
    }

    /// Unpacks an iterator of [`Event`]s onto the filesystem.
    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap_iter(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow(), write_file_mmap))
    }

    /// Unpacks a single [`Event`] onto the filesystem.
    #[inline]
    pub fn unpack(&mut self, event: impl Borrow<Event>) -> Result<(), Error> {
        self.process(event.borrow(), write_file_default)
    }

    /// Unpacks a single [`Event`] onto the filesystem.
    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap(&mut self, event: impl Borrow<Event>) -> Result<(), Error> {
        self.process(event.borrow(), write_file_mmap)
    }

    fn process(&mut self, event: &Event, write_file: WriteFileFn) -> Result<(), Error> {
        if let Event::Object(object) = event {
            debug!("unpacking object: {object:?}");
            process_object(self.root, object, write_file)
        } else {
            Ok(())
        }
    }
}

fn process_object(root: &Path, object: &Object, write_file: WriteFileFn) -> Result<(), Error> {
    let location = root.join(verify_path(&object.location)?);
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

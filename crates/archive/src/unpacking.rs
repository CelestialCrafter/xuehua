//! Unpacking of [`Event`]s into the filesystem

use std::{
    borrow::Borrow,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

use bytes::Bytes;
use xh_reports::{compat::StdCompat, prelude::*};

use crate::{Event, Object, ObjectContent, utils::debug};

/// Error type for unpacking
#[derive(Default, Debug, IntoReport)]
#[message("could not unpack archive")]
pub struct Error;

// TODO: impl overwrite option
/// Packer for archive events.
///
/// The unpacker consumes [`Event`]s and unpacks them to the filesystem.
pub struct Unpacker<'a> {
    root: &'a Path,
}

type WriteFileFn = fn(&Path, &Bytes) -> StdResult<(), std::io::Error>;

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
            .try_for_each(|event| self.process(event.borrow(), write_file_default))
    }

    /// Unpacks an iterator of [`Event`]s onto the filesystem.
    ///
    /// # Safety
    ///
    /// See [`memmap2::Mmap`] for why this function is unsafe.
    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap_iter(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| unsafe { self.unpack_mmap(event.borrow()) })
    }

    /// Unpacks a single [`Event`] onto the filesystem.
    #[inline]
    pub fn unpack(&mut self, event: impl Borrow<Event>) -> Result<(), Error> {
        self.process(event.borrow(), write_file_default)
    }

    /// Unpacks a single [`Event`] onto the filesystem.
    ///
    /// # Safety
    ///
    /// See [`memmap2::Mmap`] for why this function is unsafe.
    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap(&mut self, event: impl Borrow<Event>) -> Result<(), Error> {
        self.process(event.borrow(), write_file_mmap)
    }

    fn process(&mut self, event: &Event, write_file: WriteFileFn) -> Result<(), Error> {
        if let Event::Object(object) = event {
            debug!("unpacking object: {object:?}");
            process_object(self.root, object, write_file).wrap()
        } else {
            Ok(())
        }
    }
}

fn process_object(root: &Path, object: &Object, write_file: WriteFileFn) -> Result<(), Error> {
    let location = xh_common::safe_path(root, object.location.as_ref()).wrap()?;
    debug!("unpacking to {}", location.display());

    let set_permissions =
        || fs::set_permissions(&location, fs::Permissions::from_mode(object.permissions));

    match &object.content {
        ObjectContent::File { data } => {
            write_file(&location, &data).and_then(|()| set_permissions())
        }
        ObjectContent::Symlink { target } => symlink(target, &location),
        ObjectContent::Directory => fs::create_dir(&location).and_then(|()| set_permissions()),
    }
    .compat()
    .wrap()?;

    Ok(())
}

fn write_file_default(path: &Path, contents: &Bytes) -> StdResult<(), std::io::Error> {
    fs::write(path, contents).map_err(Into::into)
}

#[cfg(feature = "mmap")]
fn write_file_mmap(path: &Path, contents: &Bytes) -> StdResult<(), std::io::Error> {
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

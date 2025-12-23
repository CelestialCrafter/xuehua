use std::{
    borrow::{Borrow, Cow},
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    Event, Object, Operation, PathBytes,
    utils::{PathEscapeError, debug, resolve_path},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: \"{event:?}\" ({reason})")]
    UnexpectedEvent {
        event: Event,
        reason: Cow<'static, str>,
    },
    #[error(transparent)]
    PathEscape(#[from] PathEscapeError),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

// TODO: impl overwrite option
pub struct Unpacker<'a> {
    index: Option<BTreeSet<PathBytes>>,
    root: &'a Path,
}

type WriteFileFn = fn(&Path, &Bytes) -> Result<(), Error>;

impl<'a> Unpacker<'a> {
    #[inline]
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            index: Default::default(),
        }
    }

    #[inline]
    pub fn unpack(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow(), write_file_default))
    }

    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn unpack_mmap(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow(), write_file_mmap))
    }

    fn process_operation(
        &self,
        dest: PathBytes,
        operation: &Operation,
        write_file: WriteFileFn,
    ) -> Result<(), Error> {
        let dest = resolve_path(self.root, &dest)?;

        debug!("unpacking to {}", dest.display());

        match operation {
            Operation::Create {
                permissions,
                object,
            } => {
                match object {
                    Object::File { contents } => write_file(&dest, contents)?,
                    Object::Symlink { target } => symlink(resolve_path(self.root, target)?, &dest)?,
                    Object::Directory => fs::create_dir(&dest)?,
                };

                if let Object::File { .. } | Object::Directory = object {
                    fs::set_permissions(&dest, fs::Permissions::from_mode(*permissions))?;
                }
            }
            Operation::Delete => {
                if fs::symlink_metadata(&dest)?.is_dir() {
                    fs::remove_dir_all(&dest)
                } else {
                    fs::remove_file(&dest)
                }?;
            }
        }

        Ok(())
    }

    fn process(&mut self, event: &Event, write_file: WriteFileFn) -> Result<(), Error> {
        debug!("unpacking {event:?}");

        match self.index {
            None => match event {
                Event::Index(index) => {
                    self.index = Some(index.clone());
                    Ok(())
                }
                _ => Err(Error::UnexpectedEvent {
                    event: event.clone(),
                    reason: "expected index".into(),
                }),
            },
            Some(ref mut index) => {
                let Event::Operation(operation) = event else {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        reason: "expected operation".into(),
                    });
                };

                let dest = index.pop_first().ok_or_else(|| Error::UnexpectedEvent {
                    event: event.clone(),
                    reason: "too many events".into(),
                })?;

                self.process_operation(dest, operation, write_file)
            }
        }
    }
}

fn write_file_default(path: &Path, contents: &Bytes) -> Result<(), Error> {
    fs::write(path, contents).map_err(Into::into)
}

#[cfg(feature = "mmap")]
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

    map.copy_from_slice(&contents);

    Ok(())
}

use std::{
    borrow::{Borrow, Cow},
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Component, Path, PathBuf},
};

use bytes::Bytes;
use thiserror::Error;

use crate::{Event, Index, Object, ObjectMetadata, PathBytes, utils::debug};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: {event:?} ({reason})")]
    UnexpectedEvent {
        event: Event,
        reason: Cow<'static, str>,
    },
    #[error("invalid path: {0:?}")]
    InvalidPath(PathBytes),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

// TODO: impl overwrite option
pub struct Unpacker<'a> {
    index: Option<Index>,
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

    fn process(&mut self, event: &Event, write_file: WriteFileFn) -> Result<(), Error> {
        debug!("unpacking {event:?}");

        match self.index {
            None => {
                let Event::Index(index) = event else {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        reason: "expected index".into(),
                    });
                };

                self.index = Some(index.clone());
                Ok(())
            }
            Some(ref mut index) => {
                let Event::Object(object) = event else {
                    return Err(Error::UnexpectedEvent {
                        event: event.clone(),
                        reason: "expected object".into(),
                    });
                };

                let (path, metadata) = index.pop_first().ok_or_else(|| Error::UnexpectedEvent {
                    event: event.clone(),
                    reason: "too many events".into(),
                })?;

                self.process_object(
                    self.root.join(verify_path(&path)?),
                    metadata,
                    object,
                    write_file,
                )
            }
        }
    }

    fn process_object(
        &self,
        path: PathBuf,
        metadata: ObjectMetadata,
        object: &Object,
        write_file: WriteFileFn,
    ) -> Result<(), Error> {
        debug!("unpacking to {}", path.display());

        match object {
            Object::File { contents } => write_file(path.as_ref(), contents)?,
            Object::Symlink { target } => symlink(target, &path)?,
            Object::Directory => fs::create_dir(&path)?,
        };

        if let Object::File { .. } | Object::Directory = object {
            fs::set_permissions(&path, fs::Permissions::from_mode(metadata.permissions))?;
        }

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

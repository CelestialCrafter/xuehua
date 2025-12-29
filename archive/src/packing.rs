//! Packing of [`Event`]s from the filesystem

use std::{collections::VecDeque, fs, os::unix::fs::PermissionsExt, path::Path};

use bytes::Bytes;
use thiserror::Error;

use crate::{Object, ObjectContent, PathBytes, utils::debug};

/// Error type for packing
#[derive(Error, Debug)]
pub enum Error {
    /// A file had a type that could not be packed
    #[error("unsupported file type at {0:?}")]
    UnsupportedType(PathBytes),
    #[allow(missing_docs)]
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

type ReadFileFn = fn(&Path) -> Result<Bytes, Error>;

// TODO: make packer stateless
/// Packer for archive events
///
/// The packer walks a directory tree, and outputs [`Event`]s.
pub struct Packer {
    index: Option<VecDeque<Object>>,
    root: PathBytes,
}

impl Packer {
    /// Constructs a new packer.
    pub fn new(root: impl Into<PathBytes>) -> Self {
        Self {
            index: None,
            root: root.into(),
        }
    }

    /// Packs a directory into into an iterator of [`Event`]s.
    pub fn pack(&mut self) -> impl Iterator<Item = Result<Object, Error>> {
        self.process_all(read_file_default)
    }

    #[cfg(feature = "mmap")]
    pub unsafe fn pack_mmap(&mut self) -> impl Iterator<Item = Result<Object, Error>> {
        self.process_all(read_file_mmap)
    }

    fn process_all(
        &mut self,
        read_file: ReadFileFn,
    ) -> impl Iterator<Item = Result<Object, Error>> {
        std::iter::from_fn(move || {
            if let None = self.index {
                if let Err(err) = self.build_index() {
                    return Some(Err(err));
                }
            }

            let Some(ref mut index) = self.index else {
                unreachable!("index should be built");
            };

            let object = process(self.root.as_ref(), index.front_mut()?, read_file)
                .map(|_| index.pop_front().unwrap());
            Some(object)
        })
    }

    fn build_index(&mut self) -> Result<(), Error> {
        let mut queue = Vec::from([(self.root.clone(), fs::symlink_metadata(&self.root)?)]);

        let mut i = 0;
        while let Some((path, ty)) = queue.get(i) {
            i += 1;

            if !ty.is_dir() {
                continue;
            }

            queue.extend(
                fs::read_dir(path)?
                    .map(|entry| {
                        let entry = entry?;
                        let path = entry.path();
                        let metadata = fs::symlink_metadata(&path)?;

                        Ok((path.into(), metadata))
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            );
        }

        let mut index: Vec<_> = queue
            .into_iter()
            // skip root dir
            .skip(1)
            .map(|(location, metadata)| {
                let content = if metadata.is_file() {
                    ObjectContent::File { data: Bytes::new() }
                } else if metadata.is_symlink() {
                    ObjectContent::Symlink {
                        target: Bytes::new().into(),
                    }
                } else if metadata.is_dir() {
                    ObjectContent::Directory
                } else {
                    return Err(Error::UnsupportedType(location));
                };

                Ok(Object {
                    permissions: metadata.permissions().mode(),
                    location,
                    content,
                })
            })
            .collect::<Result<_, _>>()?;
        index.sort_unstable_by(|a, b| a.location.cmp(&b.location));
        self.index = Some(index.into());

        Ok(())
    }
}

fn process(root: &Path, stub: &mut Object, read_file: ReadFileFn) -> Result<(), Error> {
    let location = stub.location.as_ref();
    debug!("packing {}", location.display());

    let content = match stub.content {
        ObjectContent::File { .. } => ObjectContent::File {
            data: read_file(&location)?,
        },
        ObjectContent::Symlink { .. } => ObjectContent::Symlink {
            target: fs::read_link(location)?.into(),
        },
        ObjectContent::Directory => ObjectContent::Directory,
    };

    let location = location
        .strip_prefix(&root)
        .expect("path should be a child of root")
        .to_path_buf()
        .into();

    stub.location = location;
    stub.content = content;
    Ok(())
}

#[inline]
fn read_file_default(path: &Path) -> Result<Bytes, Error> {
    match fs::read(path) {
        Ok(data) => Ok(data.into()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(feature = "mmap")]
#[inline]
fn read_file_mmap(path: &Path) -> Result<Bytes, Error> {
    let file = fs::File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    mmap.advise(memmap2::Advice::Sequential)?;

    Ok(Bytes::from_owner(mmap))
}

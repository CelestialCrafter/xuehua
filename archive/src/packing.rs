use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use bytes::Bytes;
use thiserror::Error;

use crate::{Event, Index, Object, ObjectMetadata, ObjectType, PathBytes, utils::debug};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unsupported file type at {0:?}")]
    UnsupportedType(PathBytes),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

type ReadFileFn = fn(&PathBytes) -> Result<Bytes, Error>;

pub struct Packer {
    index: Option<Index>,
    root: PathBytes,
}

impl Packer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            index: None,
            root: root.into(),
        }
    }

    pub fn pack(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        self.process_all(read_file_default)
    }

    #[cfg(feature = "mmap")]
    pub unsafe fn pack_mmap(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        self.process_all(read_file_mmap)
    }

    fn process_all(&mut self, read_file: ReadFileFn) -> impl Iterator<Item = Result<Event, Error>> {
        std::iter::from_fn(move || {
            Some(match self.index {
                Some(ref mut index) => {
                    let (path, metadata) = index.pop_first()?;
                    self.process(path, metadata, read_file)
                }
                None => build_index(&self.root).map(|(internal, external)| {
                    self.index = Some(internal);
                    Event::Index(external)
                }),
            })
        })
    }

    fn process(
        &self,
        path: PathBytes,
        metadata: ObjectMetadata,
        read_file: ReadFileFn,
    ) -> Result<Event, Error> {
        debug!("packing {}", path.as_ref().display());

        Ok(Event::Object(match metadata.variant {
            ObjectType::File => Object::File {
                contents: read_file(&path)?,
            },
            ObjectType::Symlink => Object::Symlink {
                target: fs::read_link(path)?.into(),
            },
            ObjectType::Directory => Object::Directory,
        }))
    }
}

fn build_index(root: &PathBytes) -> Result<(Index, Index), Error> {
    let mut queue = Vec::from([(root.as_ref().to_path_buf(), fs::symlink_metadata(root)?)]);

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
                    Ok((entry.path(), entry.metadata()?))
                })
                .collect::<Result<Vec<_>, Error>>()?,
        );
    }

    let (internal, external) = queue
        .into_iter()
        // skip root dir
        .skip(1)
        .map(|(path, metadata)| {
            let stripped = path
                .strip_prefix(root)
                .expect("path should be a child of root")
                .to_path_buf()
                .into();
            let path = path.into();

            let variant = if metadata.is_file() {
                ObjectType::File
            } else if metadata.is_symlink() {
                ObjectType::Symlink
            } else if metadata.is_dir() {
                ObjectType::Directory
            } else {
                return Err(Error::UnsupportedType(path));
            };

            let size = if let ObjectType::Directory = variant {
                0
            } else {
                metadata.len()
            };

            let metadata = ObjectMetadata {
                permissions: metadata.permissions().mode(),
                size,
                variant,
            };

            Ok(((path, metadata), (stripped, metadata)))
        })
        .collect::<Result<_, _>>()?;

    Ok((internal, external))
}

#[inline]
fn read_file_default(path: &PathBytes) -> Result<Bytes, Error> {
    Ok(fs::read(path)?.into())
}

#[cfg(feature = "mmap")]
#[inline]
fn read_file_mmap(path: &PathBytes) -> Result<Bytes, Error> {
    let file = fs::File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    mmap.advise(memmap2::Advice::Sequential)?;

    Ok(Bytes::from_owner(mmap))
}

use std::{
    collections::BTreeSet,
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::PathBuf,
};

use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

use crate::{
    Event, Object, Operation, PathBytes,
    utils::{PathEscapeError, debug, resolve_path},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unsupported file type at {0:?}")]
    UnsupportedType(PathBytes),
    #[error(transparent)]
    PathEscape(#[from] PathEscapeError),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub follow_symlinks: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
        }
    }
}

pub struct Packer {
    index: Option<BTreeSet<PathBytes>>,
    root: PathBytes,
    options: Options,
}

type ReadFileFn = fn(&PathBytes) -> Result<Bytes, Error>;

impl Packer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            index: None,
            options: Default::default(),
            root: root.into(),
        }
    }

    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
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
                    let path = index.pop_first()?;
                    self.process(&path, read_file)
                }
                None => build_index(&self.root).map(|(internal, external)| {
                    self.index = Some(internal);
                    Event::Index(external)
                }),
            })
        })
    }

    fn process(&self, path: &PathBytes, read_file: ReadFileFn) -> Result<Event, Error> {
        debug!("packing {}", path.as_ref().display());

        let metadata = if self.options.follow_symlinks {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }?;
        let permissions = metadata.permissions().mode();

        let event = if metadata.is_dir() {
            Event::Operation(Operation::Create {
                permissions,
                object: Object::Directory,
            })
        } else if metadata.is_file() {
            Event::Operation(Operation::Create {
                permissions,
                object: Object::File {
                    contents: read_file(path)?,
                },
            })
        } else if metadata.is_symlink() {
            let target = fs::read_link(path)?.into();
            resolve_path(&self.root, &target)?;

            Event::Operation(Operation::Create {
                permissions,
                object: Object::Symlink { target },
            })
        } else {
            return Err(Error::UnsupportedType(path.clone()));
        };

        Ok(event)
    }
}

fn build_index(root: &PathBytes) -> Result<(BTreeSet<PathBytes>, BTreeSet<PathBytes>), Error> {
    let mut queue = Vec::from([(root.clone(), fs::symlink_metadata(root)?.file_type())]);

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
                    Ok((entry.path().into(), entry.file_type()?))
                })
                .collect::<Result<Vec<_>, Error>>()?,
        );
    }

    let internal: BTreeSet<_> = queue.into_iter().map(|(path, _)| path).collect();

    let base = fs::canonicalize(root)?;
    let external = internal
        .iter()
        .map(|path| {
            let mut stripped = BytesMut::new();
            stripped.put_u8(b'/');
            stripped.put_slice(
                path.as_ref()
                    .strip_prefix(&base)
                    .expect("path should be a child of root")
                    .as_os_str()
                    .as_bytes(),
            );

            stripped.freeze().into()
        })
        .collect();

    Ok((internal, external))
}

fn read_file_default(path: &PathBytes) -> Result<Bytes, Error> {
    Ok(fs::read(path)?.into())
}

#[cfg(feature = "mmap")]
fn read_file_mmap(path: &PathBytes) -> Result<Bytes, Error> {
    let file = fs::File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    Ok(Bytes::from_owner(mmap))
}

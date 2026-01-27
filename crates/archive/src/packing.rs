//! Packing of [`Event`]s from the filesystem

use std::{collections::VecDeque, fs, os::unix::fs::PermissionsExt, path::Path};

use bytes::Bytes;
use xh_reports::{compat::StdCompat, prelude::*};

use crate::{Event, Object, ObjectContent, PathBytes, utils::debug};

/// An unsupported file type was encountered (eg. socket, pipe, etc)
#[derive(Debug, IntoReport)]
#[message("unsupported file type")]
#[context(debug: path)]
pub struct UnsupportedTypeError {
    path: PathBytes,
}

/// Error type for packing
#[derive(Default, Debug, IntoReport)]
#[message("could not pack archive")]
pub struct Error;

type ReadFileFn = fn(&Path) -> StdResult<Bytes, std::io::Error>;

enum State {
    Header,
    Objects(VecDeque<Object>),
    Footer,
}

/// Packer for archive events
///
/// The packer walks a directory tree, and outputs [`Event`]s.
pub struct Packer {
    state: State,
    root: PathBytes,
}

impl Packer {
    /// Constructs a new packer.
    #[inline]
    pub fn new(root: impl Into<PathBytes>) -> Self {
        Self {
            state: State::Header,
            root: root.into(),
        }
    }

    /// Packs a directory into an iterator of [`Event`]s.
    #[inline]
    pub fn pack_iter(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        std::iter::from_fn(|| self.process(read_file_default))
    }

    /// Packs a directory into an iterator of [`Event`]s.
    ///
    /// # Safety
    ///
    /// See [`memmap2::Mmap`] for why this function is unsafe.
    #[cfg(feature = "mmap")]
    #[inline]
    pub unsafe fn pack_mmap_iter(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        std::iter::from_fn(|| self.process(read_file_mmap))
    }

    fn process(&mut self, read_file: ReadFileFn) -> Option<Result<Event, Error>> {
        Some(match self.state {
            State::Header => build_index(&self.root).map(|index| {
                self.state = State::Objects(index);
                Event::Header
            }),
            State::Objects(ref mut index) => match index.front_mut() {
                Some(stub) => process_object(&self.root, stub, read_file)
                    .map(|_| Event::Object(index.pop_front().unwrap())),
                None => {
                    self.state = State::Footer;
                    Ok(Event::Footer(Default::default()))
                }
            },
            State::Footer => return None,
        })
    }
}

fn process_object(root: &PathBytes, stub: &mut Object, read_file: ReadFileFn) -> Result<(), Error> {
    let location = stub.location.as_ref();
    debug!("packing {}", location.display());

    let content = match stub.content {
        ObjectContent::File { .. } => ObjectContent::File {
            data: read_file(&location).compat().wrap()?,
        },
        ObjectContent::Symlink { .. } => ObjectContent::Symlink {
            target: fs::read_link(location).compat().wrap()?.into(),
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

fn build_index(root: &PathBytes) -> Result<VecDeque<Object>, Error> {
    let mut queue = Vec::from([(root.clone(), fs::symlink_metadata(&root).compat().wrap()?)]);

    let mut i = 0;
    while let Some((path, ty)) = queue.get(i) {
        i += 1;

        if !ty.is_dir() {
            continue;
        }

        queue.extend(
            fs::read_dir(path)
                .wrap()?
                .map(|entry| {
                    let entry = entry?;
                    let path = entry.path();
                    let metadata = fs::symlink_metadata(&path).compat()?;

                    Ok((path.into(), metadata))
                })
                .collect::<Result<Vec<_>, _>>()
                .wrap()?,
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
                return Err(UnsupportedTypeError { path: location }.wrap());
            };

            Ok(Object {
                permissions: metadata.permissions().mode(),
                location,
                content,
            })
        })
        .collect::<Result<_, _>>()?;
    index.sort_unstable_by(|a, b| a.location.cmp(&b.location));

    Ok(index.into())
}

fn read_file_default(path: &Path) -> StdResult<Bytes, std::io::Error> {
    Ok(fs::read(path)?.into())
}

#[cfg(feature = "mmap")]
fn read_file_mmap(path: &Path) -> StdResult<Bytes, std::io::Error> {
    let file = fs::File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file) }?;
    mmap.advise(memmap2::Advice::Sequential)?;

    Ok(Bytes::from_owner(mmap))
}

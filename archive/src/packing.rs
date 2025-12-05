use std::{
    collections::{BTreeMap, VecDeque},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    },
    path::{Path, PathBuf},
};

use bytes::Bytes;
use thiserror::Error;

use crate::{Contents, Event, Object, Operation, utils::debug};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

#[derive(Clone, Copy)]
pub struct Options {
    pub follow_symlinks: bool,
    pub symlink_escapes: bool,
    pub retry: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
            retry: false,
            symlink_escapes: false,
        }
    }
}

pub struct Packer {
    queue: VecDeque<PathBuf>,
    root: PathBuf,
    diff: BTreeMap<PathBuf, Operation>,
    options: Options,
}

impl Packer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            queue: VecDeque::from([root.clone()]),
            diff: Default::default(),
            options: Default::default(),
            root,
        }
    }

    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    pub fn with_diff(mut self, iterator: impl Iterator<Item = Event>) -> Self {
        todo!()
    }

    pub fn pack(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        let mut attempt = PathBuf::with_capacity(64);
        std::iter::from_fn(move || {
            self.queue.front()?.clone_into(&mut attempt);

            let processed = self.process(&attempt);
            match processed {
                Err(_) if self.options.retry => (),
                _ => {
                    self.queue.pop_front();
                }
            }

            Some(processed)
        })
    }

    fn process(&mut self, path: &Path) -> Result<Event, Error> {
        debug!("packing {}", path.display());

        let metadata = if self.options.follow_symlinks {
            std::fs::metadata(&path)
        } else {
            std::fs::symlink_metadata(&path)
        }?;

        let path_to_pathbytes =
            |path: PathBuf| Bytes::from_owner(path.into_os_string().into_vec()).into();
        let mode = || metadata.permissions().mode();

        let event = if metadata.is_dir() {
            let mut back = PathBuf::with_capacity(64);
            let mut last_file = 0;

            loop {
                let next = self.queue.back().unwrap();
                if *next == back {
                    break;
                }
                next.clone_into(&mut back);

                let entries = std::fs::read_dir(next)?;
                let (lower, _) = entries.size_hint();
                self.queue.reserve(lower);

                for entry in entries {
                    let entry = entry?;
                    let ty = entry.file_type()?;
                    let child = entry.path();

                    if ty.is_dir() {
                        self.queue.push_back(child);
                    } else {
                        self.queue.push_front(child);
                        last_file += 1;
                    }
                }
            }

            self.queue.truncate(last_file);
            Event::Index(
                self.queue
                    .iter()
                    .map(|path| {
                        let mut normalized = vec![b'/'];
                        normalized.extend_from_slice(
                            path.strip_prefix(&self.root)
                                .expect("path should be a prefix of root")
                                .as_os_str()
                                .as_bytes(),
                        );
                        debug!("root: {}, path: {}", self.root.display(), path.display());
                        Bytes::from_owner(normalized).into()
                    })
                    .collect(),
            )
        } else if metadata.is_file() {
            Event::Operation(Operation::Create {
                permissions: mode(),
                object: Object::File {
                    prefix: None,
                    contents: Contents::Uncompressed(std::fs::read(path)?.into()),
                },
            })
        } else if metadata.is_symlink() {
            if !self.options.symlink_escapes {
                todo!("symlink escape checks");
            }

            Event::Operation(Operation::Create {
                permissions: mode(),
                object: Object::Symlink {
                    target: path_to_pathbytes(std::fs::read_link(path)?),
                },
            })
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported file kind",
            )
            .into());
        };

        Ok(event)
    }
}

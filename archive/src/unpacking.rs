use std::{
    borrow::{Borrow, Cow},
    collections::BTreeSet,
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::Path,
};

use thiserror::Error;

use crate::{Contents, Event, Object, Operation, PathBytes, utils::debug};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: \"{event:?}\" ({reason})")]
    UnexpectedEvent {
        event: Event,
        reason: Cow<'static, str>,
    },
    #[error("file contents should be uncompressed")]
    Compressed,
    #[error("path should be absolute")]
    RelativePath(PathBytes),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

#[derive(Clone, Copy)]
pub struct Options {
    pub follow_symlinks: bool,
    pub disable_sandbox: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
            disable_sandbox: false,
        }
    }
}

pub struct Unpacker<'a> {
    index: Option<BTreeSet<PathBytes>>,
    root: &'a Path,
    options: Options,
}

impl<'a> Unpacker<'a> {
    #[inline]
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            index: Default::default(),
            options: Default::default(),
        }
    }

    #[inline]
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    #[inline]
    pub fn unpack(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow()))
    }

    fn process(&mut self, event: &Event) -> Result<(), Error> {
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

                if !self.options.disable_sandbox {
                    todo!("unpacker fs sandbox");
                }

                let path = index.pop_first().ok_or_else(|| Error::UnexpectedEvent {
                    event: event.clone(),
                    reason: "too many events".into(),
                })?;
                let path = path
                    .as_ref()
                    .strip_prefix("/")
                    .map_err(|_| Error::RelativePath(path.clone()));
                let path = self.root.join(path?);
                debug!("unpacking to {}", path.display());

                match operation {
                    Operation::Create {
                        permissions,
                        object,
                    } => {
                        match object {
                            Object::File {
                                contents,
                                prefix: _,
                            } => match contents {
                                Contents::Compressed(_) => return Err(Error::Compressed),
                                Contents::Decompressed(contents) => std::fs::write(&path, contents),
                            },
                            // TODO: sandbox path
                            Object::Symlink { target } => {
                                if !self.options.disable_sandbox {
                                    todo!("packer fs sandbox");
                                }

                                std::os::unix::fs::symlink(self.root.join(target), &path)
                            }
                            Object::Directory => std::fs::create_dir(&path),
                        }?;

                        if let Object::File { .. } | Object::Directory = object {
                            std::fs::set_permissions(&path, Permissions::from_mode(*permissions))?;
                        }
                    }
                    Operation::Delete => {
                        let metadata = if self.options.follow_symlinks {
                            std::fs::metadata(&path)
                        } else {
                            std::fs::symlink_metadata(&path)
                        }?;

                        if metadata.is_dir() {
                            std::fs::remove_dir_all(&path)
                        } else {
                            std::fs::remove_file(&path)
                        }?;
                    }
                }

                Ok(())
            }
        }
    }
}

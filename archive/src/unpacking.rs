use std::{
    borrow::{Borrow, Cow},
    collections::BTreeSet,
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::Path,
};

use thiserror::Error;

use crate::{Contents, Event, Object, Operation, PathBytes};

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
    follow_symlinks: bool,
    disable_sandbox: bool,
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
    pub fn pack(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow()))
    }

    fn process(&mut self, event: &Event) -> Result<(), Error> {
        eprintln!("processing {event:?}");

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
                    todo!("packer fs sandbox");
                }

                let path = index.pop_first().ok_or_else(|| Error::UnexpectedEvent {
                    event: event.clone(),
                    reason: "too many events".into(),
                })?;
                let path = path
                    .strip_prefix("/")
                    .map_err(|_| Error::RelativePath(path.clone()));
                let path = self.root.join(path?);

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
                                Contents::Uncompressed(contents) => std::fs::write(&path, contents),
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::Path};

    use blake3::Hash;
    use bytes::Bytes;

    use crate::{Contents, Event, Object, Operation, PathBytes, unpacking::Unpacker};

    #[test]
    fn test_packing() {
        let events = [
            Event::Index(BTreeSet::from([
                PathBytes {
                    inner: Bytes::from_static(b"/a"),
                },
                PathBytes {
                    inner: Bytes::from_static(b"/b"),
                },
                PathBytes {
                    inner: Bytes::from_static(b"/c"),
                },
                PathBytes {
                    inner: Bytes::from_static(b"/d"),
                },
            ])),
            Event::Operation(Operation::Create {
                permissions: 0o755,
                object: Object::File {
                    contents: Contents::Uncompressed(Bytes::from_static(b"im wauwa")),
                    prefix: Some(Hash::from_bytes([1; 32])),
                },
            }),
            Event::Operation(Operation::Create {
                permissions: 0o755,
                object: Object::Symlink {
                    target: PathBytes {
                        inner: Bytes::from_static(b"/e"),
                    },
                },
            }),
            Event::Operation(Operation::Create {
                permissions: 0o644,
                object: Object::Directory,
            }),
            Event::Operation(Operation::Delete),
        ];

        Unpacker::new(Path::new("test/"))
            .pack(events)
            .expect("packer should pack");
    }
}

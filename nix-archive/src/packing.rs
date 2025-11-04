//! Read the filesystem into NAR events
//!
//! # Examples
//!
//! Packs the working directory into a NAR on stdout
//!
//! ```rust,no_run
//! use nix_archive::{packing::{Packer, Options}, encoding::Encoder};
//! use std::path::PathBuf;
//!
//! let packer = Packer::new(
//!     PathBuf::from("."),
//!     Options::default()
//! );
//!
//! let mut output = bytes::BytesMut::new();
//! Encoder::new().encode(&mut output, packer.pack())?;
//! # Ok::<_, anyhow::Error>(())
//! ```

use std::{
    fs,
    io::{self, Read},
    os::unix::{
        ffi::OsStringExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use bytes::Bytes;
use thiserror::Error;

use crate::Event;

/// Error type for the [Packer]
#[derive(Error, Debug)]
pub enum Error {
    /// Usually due to an error from the filesystem
    #[error(transparent)]
    IOError(#[from] io::Error),
}

/// Options for packing NARs
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Should the packer traverse symlinks?
    pub follow_symlinks: bool,
}

/// "Packs" the filesystem into NARs
///
/// See the [module-level documentation](self) for more.
pub struct Packer {
    root: PathBuf,
    options: Options,
}

enum Operation {
    Discover(PathBuf),
    Read(fs::File),
    Emit(Event),
}

impl Packer {
    /// Constructs a new [Packer]
    pub fn new(root: PathBuf, options: Options) -> Self {
        Self { root, options }
    }

    fn discovery(&mut self, queue: &mut Vec<Operation>, path: &Path) -> Result<(), Error> {
        let metadata = if self.options.follow_symlinks {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }?;

        if metadata.is_file() {
            let file = fs::File::open(&path)?;

            queue.push(Operation::Read(file));

            let executable = metadata.permissions().mode() & 0o111 != 0;

            queue.push(Operation::Emit(Event::Regular {
                executable,
                size: metadata.size(),
            }));
        } else if metadata.is_symlink() {
            queue.push(Operation::Emit(Event::Symlink {
                target: fs::read_link(&path)?.into_os_string().into_vec().into(),
            }));
        } else if metadata.is_dir() {
            let mut entries = fs::read_dir(&path)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_unstable_by_key(|e| e.file_name());

            // reverse insertions since queue is LIFO
            queue.push(Operation::Emit(Event::DirectoryEnd));

            for entry in entries.into_iter().rev() {
                queue.push(Operation::Discover(entry.path()));
                queue.push(Operation::Emit(Event::DirectoryEntry {
                    name: entry.file_name().into_vec().into(),
                }));
            }

            queue.push(Operation::Emit(Event::Directory));
        } else {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "unsupported file type").into());
        }

        Ok(())
    }

    /// Creates an iterator that traverses the filesystem and yields NAR events
    pub fn pack(&mut self) -> impl Iterator<Item = Result<Event, Error>> {
        let mut queue = Vec::from([Operation::Discover(self.root.clone())]);
        let mut buffer = vec![0u8; 8192];

        std::iter::from_fn(move || {
            loop {
                let op = match queue.pop() {
                    Some(op) => op,
                    None => break None,
                };

                match op {
                    Operation::Emit(event) => break Some(Ok(event)),
                    Operation::Discover(ref path) => {
                        if let Err(err) = self.discovery(&mut queue, &path) {
                            break Some(Err(err));
                        }
                    }
                    Operation::Read(mut file) => match file.read(&mut buffer) {
                        Ok(0) => (),
                        Ok(n) => {
                            queue.push(Operation::Read(file));
                            queue.push(Operation::Emit(Event::RegularContentChunk(
                                Bytes::copy_from_slice(&buffer[..n]),
                            )));
                        }
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                            queue.push(Operation::Read(file))
                        }
                        Err(err) => break Some(Err(err.into())),
                    },
                }
            }
        })
    }
}

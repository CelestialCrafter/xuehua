//! Apply NAR events onto the filesystem
//!
//! # Examples
//!
//! Unpacks a NAR from stdin into the working directory
//!
//! ```rust,no_run
//! use nix_archive::{decoding::Decoder, unpacking::Unpacker};
//! use std::io::{Read, stdin};
//!
//! let mut buffer = Vec::new();
//! stdin().read_to_end(&mut buffer)?;
//!
//! let events = Decoder::new()
//!     .decode(&mut buffer.into())
//!     .collect::<Result<Vec<_>, _>>()?;
//!
//! Unpacker::new().unpack(
//!     std::env::current_dir()?,
//!     events
//! )?;
//!
//! # Ok::<_, anyhow::Error>(())
//! ```

use std::{
    ffi::{OsStr, OsString},
    fs::{File, Permissions, create_dir},
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{PermissionsExt, symlink},
    },
    path::{Component, Path, PathBuf},
};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    Event,
    validation::{Error as ValidationError, EventValidator},
};

/// Error type for the [Unpacker]
#[derive(Error, Debug)]
pub enum Error {
    /// A directory entry attempted to escape from the root
    #[error("entry {0:?} attempted escape from root")]
    AttemptedEscape(Bytes),
    /// The internal validator errored
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
    /// Usually due to an error from the filesystem
    #[error(transparent)]
    IOError(#[from] io::Error),
}

fn resolve_into<'a>(
    root: &Path,
    dest: &mut PathBuf,
    components: impl Iterator<Item = Component<'a>>,
) -> bool {
    for component in components {
        match component {
            Component::Normal(name) => dest.push(name),
            Component::ParentDir => {
                if !dest.pop() {
                    return false;
                }
            }
            Component::RootDir => (),
            Component::CurDir => (),
            Component::Prefix(_) => (),
        }
    }

    dest.starts_with(root)
}

/// Options for unpacking NARs
///
/// When using [Default::default], the strictest settings
/// are enabled, allowing no escapes from the root
#[derive(Default, Clone, Copy, Debug)]
pub struct Options {
    /// Are existing files allowed to be overriden?
    /// This option does not affect symlinks
    pub overwrite: bool,
}

/// "Unpacks" NARs into the filesystem
///
/// See the [module-level documentation](self) for more.
#[derive(Clone, Debug)]
pub struct Unpacker {
    options: Options,
    validator: EventValidator,
    root: PathBuf,
}

impl Unpacker {
    /// Constructs a new [Unpacker]
    pub fn new(root: PathBuf, options: Options) -> Self {
        Self {
            options,
            root,
            validator: EventValidator::new(),
        }
    }

    /// Applies an iterator of [Events](Event) onto the filesystem
    pub fn unpack(
        &mut self,
        events: impl IntoIterator<Item = impl std::borrow::Borrow<Event>>,
    ) -> Result<(), Error> {
        let mut attempt_validator = EventValidator::new();
        let mut current_position = self.root.clone();
        let mut current_file = None;

        for event in events {
            let event = event.borrow();
            self.validator.clone_into(&mut attempt_validator);
            self.validator.advance(event)?;

            // allow the file to be dropped since we dont need it anymore
            match event {
                Event::RegularContentChunk(_) => (),
                _ => current_file = None,
            }

            match event {
                Event::Header => (),
                Event::Regular { executable, size } => {
                    let file = if self.options.overwrite {
                        File::create(&current_position)
                    } else {
                        File::create_new(&current_position)
                    }?;

                    file.set_len(*size)?;
                    file.set_permissions(Permissions::from_mode(if *executable {
                        0o755
                    } else {
                        0o644
                    }))?;

                    current_file = Some(file);
                    current_position.pop();
                }
                Event::RegularContentChunk(data) => {
                    let Some(ref mut file) = current_file else {
                        panic!("file was none during content chunk");
                    };

                    file.write_all(&data)?;
                }
                Event::Symlink { target } => {
                    let target = Path::new(OsStr::from_bytes(&target));

                    symlink(&current_position, target)?;
                    current_position.pop();
                }
                Event::Directory => {
                    if let Err(err) = create_dir(&current_position) {
                        if !self.options.overwrite || err.kind() == io::ErrorKind::AlreadyExists {
                            return Err(err.into());
                        }
                    }

                    current_position.pop();
                }
                Event::DirectoryEntry { name } => {
                    if !resolve_into(
                        &self.root,
                        &mut current_position,
                        Path::new(OsStr::from_bytes(&name)).components(),
                    ) {
                        return Err(Error::AttemptedEscape(name.clone()));
                    }
                }
                Event::DirectoryEnd => {
                    current_position.pop();
                }
            };
        }

        Ok(())
    }
}

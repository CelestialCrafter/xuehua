//! Apply NAR events onto the filesystem
//!
//! # Examples
//!
//! Unpacks a NAR from stdin into ./unpacked
//!
//! ```rust,no_run
//! use nix_archive::{decoding::Decoder, unpacking::Unpacker};
//! # #[derive(thiserror::Error, Debug)]
//! # enum Error {
//! #     #[error(transparent)]
//! #     IOError(#[from] std::io::Error),
//! #     #[error(transparent)]
//! #     DecodeError(#[from] nix_archive::decoding::Error),
//! #     #[error(transparent)]
//! #     UnpackError(#[from] nix_archive::unpacking::Error),
//! # }
//!
//! let events = Decoder::new(std::io::stdin())
//!     .collect::<Result<Vec<_>, _>>()?;
//! let output = std::env::current_dir()?.join("unpacked");
//! Unpacker::default().unpack(output, events.iter())?;
//!
//! # Ok::<_, Error>(())
//! ```

use std::{
    fs::{File, create_dir},
    io::{Error as IOError, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::{Event, validation::{Error as ValidationError, EventValidator}};

// TODO: use std's normalize_lexically if/when it becomes stable
// NOTE: vendored from std at 1.91.1 ed61e7d7e 2025-11-07
fn normalize_lexically(path: &Path) -> Option<PathBuf> {
    let mut lexical = PathBuf::new();
    let mut iter = path.components().peekable();

    // Find the root, if any, and add it to the lexical path.
    // Here we treat the Windows path "C:\" as a single "root" even though
    // `components` splits it into two: (Prefix, RootDir).
    let root = match iter.peek() {
        Some(Component::ParentDir) => return None,
        Some(p @ Component::RootDir) | Some(p @ Component::CurDir) => {
            lexical.push(p);
            iter.next();
            lexical.as_os_str().len()
        }
        Some(Component::Prefix(prefix)) => {
            lexical.push(prefix.as_os_str());
            iter.next();
            if let Some(p @ Component::RootDir) = iter.peek() {
                lexical.push(p);
                iter.next();
            }
            lexical.as_os_str().len()
        }
        None => return Some(PathBuf::new()),
        Some(Component::Normal(_)) => 0,
    };

    for component in iter {
        match component {
            Component::RootDir => unreachable!(),
            Component::Prefix(_) => return None,
            Component::CurDir => continue,
            Component::ParentDir => {
                // It's an error if ParentDir causes us to go above the "root".
                if lexical.as_os_str().len() == root {
                    return None;
                } else {
                    lexical.pop();
                }
            }
            Component::Normal(path) => lexical.push(path),
        }
    }

    Some(lexical)
}

fn escapes_root(root: &Path, target: &Path) -> bool {
    normalize_lexically(root)
        .and_then(|root| {
            let target = normalize_lexically(&root.join(target))?;
            Some((root, target))
        })
        .map(|(root, target)| !target.starts_with(&root))
        .is_some()
}

/// Error type for the [Unpacker]
#[derive(Error, Debug)]
pub enum Error {
    /// A path attempted to traverse outside (or escape from) the root
    #[error("file attempted escape to {0}")]
    AttemptedEscape(PathBuf),
    /// The internal validator errored
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
    /// Usually due to an error from the filesystem
    #[error(transparent)]
    IOError(#[from] IOError),
}

/// Options for unpacking NARs
///
/// When using [Default::default], the strictest settings
/// are enabled, allowing no escapes from the root
#[derive(Default, Clone, Copy)]
pub struct Options {
    /// Is the path is allowed to escape the root?
    /// (eg. path: /root/../my-file)
    pub path_escape_root: bool,
    /// Are symlinks allowed to target outside of the root?
    /// (eg. path: /root/my-symlink, target: /my-target)
    pub symlink_escape_root: bool,
}

struct State {
    path: PathBuf,
    file: Option<File>,
    validator: EventValidator,
}

/// "Unpacks" NARs into the filesystem
///
/// See the [module-level documentation](self) for more.
#[derive(Default, Clone)]
pub struct Unpacker {
    options: Options,
}

impl Unpacker {
    /// Constructs a new [Unpacker]
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    /// Applies an iterator of [Events](Event) onto the filesystem
    pub fn unpack<'a>(
        &self,
        root: PathBuf,
        mut events: impl Iterator<Item = &'a Event>,
    ) -> Result<(), Error> {
        events
            .try_fold(
                State {
                    path: root.clone(),
                    file: None,
                    validator: EventValidator::new(),
                },
                |mut state, event| {
                    state.validator.advance(event)?;

                    // allow the file to be dropped since we dont need it anymore
                    match event {
                        Event::RegularContentChunk(_) => (),
                        _ => state.file = None,
                    }

                    match event {
                        Event::Header => (),
                        Event::Regular { executable, size } => {
                            let file = File::create_new(&state.path)?;
                            file.set_len(*size)?;
                            if *executable {
                                let mut permissions = file.metadata()?.permissions();
                                permissions.set_mode(permissions.mode() | 0o111);
                                file.set_permissions(permissions)?;
                            }

                            state.file = Some(file);
                            state.path.pop();
                        }
                        Event::RegularContentChunk(data) => {
                            let Some(ref mut file) = state.file else {
                                unreachable!("frame was not regular during content chunk");
                            };

                            file.write_all(&data)?;
                        }
                        Event::Symlink { target } => {
                            if self.options.symlink_escape_root && escapes_root(&root, target) {
                                return Err(Error::AttemptedEscape(target.to_path_buf()));
                            }

                            symlink(target, &state.path)?;
                            state.path.pop();
                        }
                        Event::Directory => create_dir(&state.path)?,
                        Event::DirectoryEntry { name } => {
                            state.path.push(name);
                            if self.options.path_escape_root && escapes_root(&root, &state.path) {
                                return Err(Error::AttemptedEscape(state.path));
                            }
                        }
                        Event::DirectoryEnd => {
                            state.path.pop();
                        }
                    };

                    Ok(state)
                },
            )
            .map(|_| ())
    }
}

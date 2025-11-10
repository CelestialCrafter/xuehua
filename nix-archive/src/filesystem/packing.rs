//! Read the filesystem into NAR [Events](crate::Event)
//!
//! # Examples
//!
//! Packs the cwd into a NAR in stdout
//!
//! ```rust,no_run
//! ```

use std::{
    collections::VecDeque,
    fs::{self, read_link},
    io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};

use thiserror::Error;

use crate::Event;

/// Error type for the [Unpacker]
#[derive(Error, Debug)]
pub enum Error {
    /// Usually due to an error from the filesystem
    #[error(transparent)]
    IOError(#[from] io::Error),
}

#[derive(Clone, Copy)]
pub struct Options {
    follow_symlinks: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
        }
    }
}

/// "Unpacks" NARs into the filesystem
///
/// See the [module-level documentation](self) for more.
#[derive(Clone)]
pub struct Unpacker {
    options: Options,
    in_queue: VecDeque<PathBuf>,
    out_queue: VecDeque<Event>,
}

impl Unpacker {
    /// Constructs a new [Unpacker]
    pub fn new(root: PathBuf) -> Self {
        Self {
            in_queue: VecDeque::from([root]),
            out_queue: Default::default(),
            options: Default::default(),
        }
    }

    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    fn unpack(&mut self) -> Result<(), Error> {
        let Some(path) = self.in_queue.pop_front() else {
            return Ok(());
        };

        let metadata = if self.options.follow_symlinks {
            path.metadata()
        } else {
            path.symlink_metadata()
        }?;

        // i wish the FileType struct was an enum..
        if metadata.is_dir() {
            let mut files = fs::read_dir(path)?
                .map(|entry| Ok::<_, Error>(entry?.path()))
                .collect::<Result<Vec<_>, _>>()?;
            files.sort_unstable();

            self.in_queue.extend(files);
        } else if metadata.is_file() {
            self.out_queue.push_back(Event::Regular {
                executable: metadata.permissions().mode() & 0o111 != 0,
                size: metadata.size(),
            });
        } else if metadata.is_symlink() {
            self.out_queue.push_back(Event::Symlink {
                target: read_link(path)?,
            });
        } else {
            return Err(io::Error::from(io::ErrorKind::Unsupported).into());
        }

        Ok(())
    }
}

impl Iterator for Unpacker {
    type Item = Result<Event, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.unpack() {
            Ok(()) => self.out_queue.pop().map(Ok),
            Err(err) => Some(Err(err)),
        }
    }
}

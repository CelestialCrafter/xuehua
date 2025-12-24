#[derive(Debug, Default, Clone, Copy)]
pub enum State {
    #[default]
    Magic,
    Index,
    Objects(u64),
}

impl State {
    #[inline]
    pub fn finished(&self) -> bool {
        matches!(self, State::Objects(amount) if *amount == 0)
    }
}

#[cfg(feature = "std")]
#[derive(thiserror::Error, Debug, Clone)]
#[error("path {0:?} attempted to escape the root")]
pub struct PathEscapeError(crate::PathBytes);

#[cfg(feature = "std")]
pub fn resolve_path<'b>(
    root: impl AsRef<std::path::Path>,
    path: &crate::PathBytes,
) -> Result<std::path::PathBuf, PathEscapeError> {
    use std::path::Component;

    let resolved = path
        .as_ref()
        .components()
        .fold(root.as_ref().to_path_buf(), |mut acc, x| {
            match x {
                Component::Prefix(..) => (),
                Component::RootDir => (),
                Component::CurDir => (),
                Component::ParentDir => {
                    acc.pop();
                }
                Component::Normal(segment) => acc.push(segment),
            };

            acc
        });

    resolved
        .starts_with(root)
        .then_some(resolved)
        .ok_or_else(|| PathEscapeError(path.clone()))
}

#[cfg(feature = "log")]
#[allow(unused_imports)]
mod log {
    pub use log::{debug, error, info, trace, warn};
}

#[cfg(not(feature = "log"))]
#[allow(unused_macros)]
#[allow(unused_imports)]
mod log {
    macro_rules! error {
        ($($x:tt)*) => {};
    }

    // warn conflicts with a builtin attribute
    macro_rules! _warn {
        ($($x:tt)*) => {};
    }

    macro_rules! info {
        ($($x:tt)*) => {};
    }

    macro_rules! debug {
        ($($x:tt)*) => {};
    }

    macro_rules! trace {
        ($($x:tt)*) => {};
    }

    pub(crate) use {_warn as warn, debug, error, info, trace};
}

pub(crate) use log::*;

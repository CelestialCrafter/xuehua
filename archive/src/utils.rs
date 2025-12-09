use blake3::Hasher;
use bytes::Bytes;

pub mod zstd {
    use thiserror::Error;

    #[derive(Error, Debug)]
    #[error("{message} (error code {code})")]
    pub struct Error {
        code: usize,
        message: &'static str,
    }

    impl From<usize> for Error {
        fn from(value: usize) -> Self {
            Error {
                code: value,
                message: zstd_safe::get_error_name(value),
            }
        }
    }

    pub const UNKNOWN_ERROR: Error = Error {
        code: 0,
        message: "unknown error",
    };
}

#[derive(Debug, Default, Clone, Copy)]
pub enum State {
    #[default]
    Magic,
    Index,
    Operations(usize),
}

pub fn hash_plen<'a>(hasher: &'a mut Hasher, bytes: &Bytes) -> &'a mut Hasher {
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(&bytes)
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

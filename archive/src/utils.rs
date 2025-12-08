use blake3::Hasher;
use bytes::Bytes;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) enum State {
    #[default]
    Magic,
    Index,
    Operations(usize),
}

pub(crate) fn hash_plen<'a>(hasher: &'a mut Hasher, bytes: &Bytes) -> &'a mut Hasher {
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

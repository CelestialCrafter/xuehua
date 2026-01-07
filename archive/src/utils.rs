use bytes::BufMut;

pub const MAGIC: &str = "xuehua-archive";
pub const VERSION: u16 = 1;

#[derive(Clone, Copy)]
pub enum Marker {
    Header,
    Footer,
    Object,
    Signature,
}

impl Marker {
    pub fn len() -> usize {
        2
    }

    pub fn put(&self, buffer: &mut impl BufMut) {
        buffer.put_slice(b"xuehua-archive@");
        buffer.put_slice(match self {
            Marker::Header => b"hd",
            Marker::Footer => b"ft",
            Marker::Object => b"ob",
            Marker::Signature => b"sg",
        });
    }
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

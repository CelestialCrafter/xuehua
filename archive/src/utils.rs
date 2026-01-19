pub(crate) use log::*;

use crate::{Object, ObjectContent};

use bytes::{BufMut, Bytes};
use xh_reports::{Frame, Report, impl_compat};

pub const MAGIC: &str = "xuehua-archive";
pub const VERSION: u16 = 1;

impl_compat!(
    ArchiveCompat,
    (bytes::TryGetError, |error| {
        let frames = [
            Frame::context("requested", error.requested),
            Frame::context("available", error.available),
            Frame::suggestion(format!(
                "provide {} more byes",
                error.requested - error.available
            )),
        ];

        Report::from_error(error).with_frames(frames)
    })
);

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

pub fn hash_object(object: &Object) -> blake3::Hash {
    fn process_lenp(hasher: &mut blake3::Hasher, bytes: &Bytes) {
        hasher
            .update(&(bytes.len() as u64).to_le_bytes())
            .update(bytes);
    }

    let mut hasher = blake3::Hasher::new();

    process_lenp(&mut hasher, &object.location.inner);
    hasher.update(&object.permissions.to_le_bytes());
    let (variant, content) = match &object.content {
        ObjectContent::File { data } => (0, data),
        ObjectContent::Symlink { target } => (1, &target.inner),
        ObjectContent::Directory => (2, &Default::default()),
    };

    hasher.update(&[variant]);
    process_lenp(&mut hasher, content);

    let hash = hasher.finalize();
    log::debug!("object hashed to {hash}");
    hash
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

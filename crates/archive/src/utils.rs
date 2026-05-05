use crate::{Object, ObjectContent};

use bytes::{BufMut, Bytes};
use smol_str::ToSmolStr;
use xh_reports::{Frame, Report, impl_compat};

pub const MAGIC: &str = "xuehua-archive";
pub const VERSION: u16 = 1;

impl_compat!(
    ArchiveCompat,
    (bytes::TryGetError, |error| {
        let frames = [
            Frame::context("requested", error.requested),
            Frame::context("available", error.available),
            Frame::suggestion(format_args!(
                "provide {} more bytes",
                error.requested - error.available
            ).to_smolstr()),
        ];

        Report::from_error(error).with_frames(frames).cast()
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

    pub fn put(self, buffer: &mut impl BufMut) {
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
        ObjectContent::Directory => (2, &Bytes::default()),
    };

    hasher.update(&[variant]);
    process_lenp(&mut hasher, content);

    let hash = hasher.finalize();
    tracing::debug!("object hashed to {hash}");
    hash
}

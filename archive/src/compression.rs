use alloc::vec::Vec;
use core::cell::OnceCell;

use bytes::Bytes;
use thiserror::Error;
use zstd_safe::CCtx;

use crate::{
    Contents, Event, Object, Operation,
    prefixes::{Error as LoaderError, PrefixLoader, unimplemented::UnimplementedLoader},
    utils::{
        debug, trace,
        zstd::{Error as ZstdError, UNKNOWN_ERROR as UNKNOWN_ZSTD_ERROR},
    },
};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ZstdError(#[from] ZstdError),
    #[error(transparent)]
    LoaderError(LoaderError),
}

pub struct Compressor<L> {
    level: u16,
    loader: L,
}

impl Compressor<UnimplementedLoader> {
    pub fn new() -> Self {
        Self {
            level: 4,
            loader: UnimplementedLoader,
        }
    }
}

impl<L: PrefixLoader> Compressor<L> {
    pub fn with_loader<T>(self, loader: T) -> Compressor<T> {
        Compressor {
            level: self.level,
            loader,
        }
    }

    pub fn with_level(mut self, level: u16) -> Self {
        self.level = level;
        self
    }

    pub fn compress(
        &mut self,
        events: impl IntoIterator<Item = Event>,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        let level = self.level;
        let make_cctx = move || {
            debug!("making new compression context");

            CCtx::try_create()
                .ok_or(UNKNOWN_ZSTD_ERROR)
                .and_then(|mut cctx| {
                    cctx.set_parameter(zstd_safe::CParameter::CompressionLevel(level as i32))?;
                    Ok(cctx)
                })
        };

        let mut global_cctx: OnceCell<CCtx<'_>> = OnceCell::new();
        let iter = events.into_iter().map(move |event| {
            let (permissions, prefix, mut contents) = match event {
                Event::Operation(Operation::Create {
                    object:
                        Object::File {
                            contents: Contents::Decompressed(contents),
                            prefix,
                        },
                    permissions,
                }) => (permissions, prefix, contents),
                _ => return Ok(event),
            };

            debug!("compressing with prefix: {prefix:?}");

            let capacity = zstd_safe::compress_bound(contents.len());
            debug!("allocating buffer of size {capacity}");

            let mut compressed = Vec::with_capacity(capacity);
            match prefix {
                Some(prefix) => {
                    let mut prefix = self.loader.load(prefix).map_err(Error::LoaderError)?;

                    // HACK: if contents and prefix point to the same underlying bytes,
                    // HACK: the prefix **will not** be used. i'm assuming this is due to some weird C stuff
                    // HACK: within zstd itsself the only workaround i've been able to find is copying the
                    // HACK: prefix/contents into a different buffer
                    if prefix.as_ptr() == contents.as_ptr() {
                        trace!("using bytes copy hack");

                        // try to minimize the impact
                        if prefix.len() > contents.len() {
                            contents = Bytes::copy_from_slice(&contents);
                        } else {
                            prefix = Bytes::copy_from_slice(&prefix);
                        }
                    }

                    let mut cctx = make_cctx()?;
                    cctx.set_parameter(zstd_safe::CParameter::EnableLongDistanceMatching(true))
                        .and_then(|_| cctx.set_pledged_src_size(Some(contents.len() as u64)))
                        .and_then(|_| cctx.ref_prefix(&prefix))
                        .and_then(|_| cctx.compress2(&mut compressed, &contents))
                }
                None => {
                    // yuck....
                    let cctx = match global_cctx.get_mut() {
                        Some(cctx) => cctx,
                        None => {
                            let _ = global_cctx.set(make_cctx()?);
                            global_cctx.get_mut().unwrap()
                        }
                    };

                    cctx.set_pledged_src_size(Some(contents.len() as u64))
                        .and_then(|_| cctx.compress2(&mut compressed, &contents))
                }
            }
            .map_err(ZstdError::from)?;

            debug!(
                "saved {:.2}% of {} bytes",
                {
                    let original = contents.len() as f64;
                    let diff = original - (compressed.len() as f64);
                    (diff / original) * 100.0
                },
                contents.len()
            );

            Ok(Event::Operation(Operation::Create {
                permissions,
                object: Object::File {
                    contents: Contents::Compressed(Bytes::from_owner(compressed)),
                    prefix,
                },
            }))
        });

        iter
    }
}

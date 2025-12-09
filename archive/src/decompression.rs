use alloc::vec::Vec;
use core::cell::OnceCell;

use bytes::Bytes;
use thiserror::Error;
use zstd_safe::DCtx;

use crate::{
    Contents, Event, Object, Operation,
    prefixes::{Error as LoaderError, PrefixLoader, unimplemented::UnimplementedLoader},
    utils::{debug, zstd::{Error as ZstdError, UNKNOWN_ERROR as UNKNOWN_ZSTD_ERROR}},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ZstdError(#[from] ZstdError),
    #[error(transparent)]
    LoaderError(LoaderError),
}

pub struct Decompressor<L> {
    loader: L,
}

impl Decompressor<UnimplementedLoader> {
    pub fn new() -> Self {
        Self {
            loader: UnimplementedLoader,
        }
    }
}

impl<L: PrefixLoader> Decompressor<L> {
    pub fn with_loader<T>(self, loader: T) -> Decompressor<T> {
        Decompressor { loader }
    }

    pub fn decompress(
        &mut self,
        events: impl IntoIterator<Item = Event>,
    ) -> impl Iterator<Item = Result<Event, Error>> {
        let make_dctx = move || {
            debug!("making new decompression context");
            DCtx::try_create().ok_or(UNKNOWN_ZSTD_ERROR)
        };

        let mut global_dctx: OnceCell<DCtx<'_>> = OnceCell::new();
        let iter = events.into_iter().map(move |event| {
            let (permissions, prefix, compressed) = match event {
                Event::Operation(Operation::Create {
                    object:
                        Object::File {
                            contents: Contents::Compressed(contents),
                            prefix,
                        },
                    permissions,
                }) => (permissions, prefix, contents),
                _ => return Ok(event),
            };

            debug!("decompressing with prefix: {prefix:?}");
            debug!("allocating buffer of size {}", contents.len());

            let mut contents = Vec::with_capacity(compressed.len());
            match prefix {
                Some(prefix) => {
                    let prefix = self.loader.load(prefix).map_err(Error::LoaderError)?;
                    let mut dctx = make_dctx()?;
                    dctx.ref_prefix(&prefix)
                        .and_then(|_| dctx.decompress(&mut contents, &compressed))
                }
                None => {
                    // yuck....
                    let dctx = match global_dctx.get_mut() {
                        Some(dctx) => dctx,
                        None => {
                            let _ = global_dctx.set(make_dctx()?);
                            global_dctx.get_mut().unwrap()
                        }
                    };

                    dctx.decompress(&mut contents, &compressed)
                }
            }
            .map_err(ZstdError::from)?;

            Ok(Event::Operation(Operation::Create {
                permissions,
                object: Object::File {
                    contents: Contents::Uncompressed(Bytes::from_owner(contents)),
                    prefix,
                },
            }))
        });

        iter
    }
}

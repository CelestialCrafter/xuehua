use alloc::vec::Vec;
use core::cell::OnceCell;

use bytes::Bytes;
use thiserror::Error;
use zstd_safe::{CCtx, OutBuffer};

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
        let iter = events.into_iter().scan(None, move |state, event| {
            let input = match event {
                event @ Event::Operation(Operation::Create { object: Object::File { prefix: Some(ref prefix) }, .. }) => {
                    let cctx = make_cctx()?;
                    cctx.set_parameter(zstd_safe::CParameter::EnableLongDistanceMatching(true))
                        .and_then(|_| cctx.ref_prefix(&prefix))

                    *state = Some((prefix, cctx));
                return Ok()
                }
                Event::Contents(Contents::Uncompressed(input)) => input,
                _ => {
                    *cctx = global_cctx;
                    return Ok(event);
                }
            };

            let mut output = Vec::with_capacity(CCtx::out_size());

            debug!("compressing with prefix: {prefix:?}");
            match prefix {
                Some(prefix) => {
                    let mut prefix = self.loader.load(prefix).map_err(Error::LoaderError)?;

                    // HACK: if contents and prefix point to the same underlying bytes,
                    // HACK: the prefix **will not** be used. i'm assuming this is due to some weird C stuff
                    // HACK: within zstd itsself the only workaround i've been able to find is copying the
                    // HACK: prefix/contents into a different buffer
                        .and_then(|_| {
                            cctx.compress_stream(&mut OutBuffer::around(&mut output), &mut input)
                        })
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

                    cctx.compress_stream(&mut OutBuffer::around(&mut output), &mut input)
                }
            }
            .map_err(ZstdError::from)?;

            debug!(
                "saved {:.2}% of {} bytes",
                {
                    let original = input.len() as f64;
                    let diff = original - (output.len() as f64);
                    (diff / original) * 100.0
                },
                input.len()
            );

            Ok(Event::Contents(Contents::Compressed(Bytes::from_owner(
                output,
            ))))
        });

        iter
    }
}

use core::borrow::Borrow;

use alloc::{string::{String, ToString}, vec::Vec};
use blake3::Hasher;
use bytes::{BufMut, Bytes};
use thiserror::Error;
use zstd_safe::CCtx;

use crate::{
    Contents, Event, Object, Operation,
    dictionary::{Dictionary, DictionaryLoader, Error as LoaderError},
    hash_plen,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected event: \"{event:?}\" ({reason})")]
    Unexpected { event: Event, reason: String },
    #[error("not enough events were processed")]
    Incomplete,
    #[error("zstd error {code}: {message}")]
    ZstdError {
        code: zstd_safe::ErrorCode,
        message: &'static str,
    },
    #[error(transparent)]
    LoaderError(LoaderError),
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Magic,
    Index,
    Operations(usize),
}

fn zstd_error(code: zstd_safe::ErrorCode) -> Error {
    Error::ZstdError {
        code,
        message: zstd_safe::get_error_name(code),
    }
}

fn zstd_error_unknown() -> Error {
    Error::ZstdError {
        code: 0,
        message: "unknown error",
    }
}

pub struct Encoder<'a, B, L> {
    state: State,
    cctx: CCtx<'a>,
    buffer: &'a mut B,
    loader: &'a mut L,
}

impl<'a, B: BufMut, L: DictionaryLoader> Encoder<'a, B, L> {
    pub fn new(buffer: &'a mut B, loader: &'a mut L) -> Result<Self, Error> {
        let mut cctx = CCtx::try_create().ok_or_else(zstd_error_unknown)?;
        cctx.set_parameter(zstd_safe::CParameter::EnableLongDistanceMatching(true))
            .map_err(zstd_error)?;
        cctx.set_parameter(zstd_safe::CParameter::CompressionLevel(14))
            .map_err(zstd_error)?;

        Ok(Self {
            state: Default::default(),
            cctx,
            buffer,
            loader,
        })
    }

    pub fn with_compression_level(mut self, level: u16) -> Result<Self, Error> {
        self.cctx
            .set_parameter(zstd_safe::CParameter::CompressionLevel(level as i32))
            .map_err(zstd_error)?;
        Ok(self)
    }

    #[inline]
    pub fn encode(
        &mut self,
        iterator: impl IntoIterator<Item = impl Borrow<Event>>,
    ) -> Result<(), Error> {
        iterator
            .into_iter()
            .try_for_each(|event| self.process(event.borrow()))
    }

    #[inline]
    pub fn finish(self) -> Result<(), Error> {
        match self.state {
            State::Magic => Err(Error::Incomplete),
            State::Index => Err(Error::Incomplete),
            State::Operations(amount) => {
                if amount > 1 {
                    Err(Error::Incomplete)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn process(&mut self, event: &Event) -> Result<(), Error> {
        match self.state {
            State::Magic => {
                self.buffer.put_slice(b"xuehua-archive");
                self.buffer.put_u16_le(1);

                self.state = State::Index;
                self.process(event)
            }
            State::Index => {
                let Event::Index(index) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "need index event".to_string(),
                    });
                };

                let mut hasher = Hasher::new();
                self.buffer.put_u64_le(index.len() as u64);
                index.iter().for_each(|path| {
                    self.put_plen(&path.inner);
                    hash_plen(&mut hasher, &path.inner);
                });
                self.buffer.put_slice(hasher.finalize().as_bytes());

                self.state = State::Operations(index.len());
                Ok(())
            }
            State::Operations(amount) => {
                let Event::Operation(operation) = event else {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "need operation event".to_string(),
                    });
                };

                if amount == 0 {
                    return Err(Error::Unexpected {
                        event: event.clone(),
                        reason: "excess event".to_string(),
                    });
                }

                match operation {
                    Operation::Create {
                        permissions,
                        object,
                        ..
                    } => self.put_create_op(*permissions, object)?,
                    Operation::Delete { .. } => self.buffer.put_u8(1),
                };

                match self.state {
                    State::Operations(ref mut amount) => *amount -= 1,
                    _ => unreachable!(),
                };

                Ok(())
            }
        }
    }

    fn put_create_op(&mut self, permissions: u32, object: &Object) -> Result<(), Error> {
        self.buffer.put_u8(1);
        self.buffer.put_u32_le(permissions);

        match object {
            Object::File {
                contents,
                dictionary,
            } => {
                self.buffer.put_u8(0);
                self.put_dictionary(&dictionary);
                match contents {
                    Contents::Compressed(bytes) => self.put_plen(bytes),
                    Contents::Uncompressed(bytes) => {
                        let compressed = self.compress_contents(dictionary, bytes)?;
                        self.put_plen(&compressed)
                    }
                }
            }
            Object::Symlink { target } => {
                self.buffer.put_u8(1);
                self.put_plen(&target.inner);
            }
            Object::Directory => self.buffer.put_u8(2),
        };

        Ok(())
    }

    fn put_dictionary(&mut self, dictionary: &Dictionary) {
        match dictionary {
            Dictionary::None => self.buffer.put_u8(0),
            Dictionary::Internal(data) => {
                self.buffer.put_u8(1);
                self.put_plen(&data);
            }
            Dictionary::External(hash) => {
                self.buffer.put_u8(2);
                self.buffer.put_slice(hash.as_bytes());
            }
        }
    }

    fn compress_contents(
        &mut self,
        dictionary: &Dictionary,
        contents: &Bytes,
    ) -> Result<Bytes, Error> {
        self.cctx
            .set_pledged_src_size(Some(contents.len() as u64))
            .map_err(zstd_error)?;

        let mut compressed = Vec::with_capacity(CCtx::out_size());
        match dictionary {
            Dictionary::None => (),
            Dictionary::Internal(bytes) => {
                self.decompress_with_dict(&bytes, contents, &mut compressed)?
            }
            Dictionary::External(id) => {
                let bytes = self.loader.load(*id).map_err(Error::LoaderError)?;
                self.decompress_with_dict(&bytes, contents, &mut compressed)?;
            }
        };

        Ok(Bytes::from_owner(compressed))
    }

    fn decompress_with_dict(
        &mut self,
        dictionary: &[u8],
        contents: &[u8],
        compressed: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.cctx.load_dictionary(dictionary).map_err(zstd_error)?;
        self.cctx
            .compress2(compressed, contents)
            .map_err(zstd_error)?;

        Ok(())
    }

    fn put_plen(&mut self, bytes: &Bytes) {
        self.buffer.put_u64_le(bytes.len() as u64);
        self.buffer.put_slice(&bytes);
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::collections::BTreeSet;

    use bytes::{Bytes, BytesMut};

    use crate::{
        Contents, Event, Object, Operation, PathBytes,
        dictionary::{Dictionary, unimplemented::UnimplementedLoader},
        encoding::Encoder,
    };

    #[test]
    fn test_encoding() {
        let mut encoded = BytesMut::new();

        let mut loader = UnimplementedLoader;
        let mut encoder =
            Encoder::new(&mut encoded, &mut loader).expect("should be able to initialize encoder");

        encoder
            .encode([
                Event::Index(BTreeSet::from([
                    PathBytes {
                        inner: Bytes::from_static(b"/file"),
                    },
                    PathBytes {
                        inner: Bytes::from_static(b"/symlink"),
                    },
                    PathBytes {
                        inner: Bytes::from_static(b"/directory"),
                    },
                    PathBytes {
                        inner: Bytes::from_static(b"/deleted"),
                    },
                ])),
                Event::Operation(Operation::Create {
                    permissions: 0o755,
                    object: Object::File {
                        contents: Contents::Uncompressed(Bytes::from_static(
                            b"this is my file, zstd compressed! <3",
                        )),
                        dictionary: Dictionary::Internal(Bytes::from_static(b"this is my file, ")),
                    },
                }),
                Event::Operation(Operation::Create {
                    permissions: 0o755,
                    object: Object::Symlink {
                        target: PathBytes {
                            inner: Bytes::from_static(b"/my/symlink/target"),
                        },
                    },
                }),
                Event::Operation(Operation::Create {
                    permissions: 0o644,
                    object: Object::Directory,
                }),
                Event::Operation(Operation::Delete),
            ])
            .expect("should be able to encode events");
        encoder.finish().expect("should be able to finish encoder");

        std::fs::write("test", encoded).expect("should be able to write to file");
    }
}

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use arbitrary::Arbitrary;
use blake3::Hash;
use bytes::{Bytes, BytesMut};
use libtest_mimic::{Failed, Measurement};
use xh_archive::compression::Compressor;
use xh_archive::decompression::Decompressor;
use xh_archive::{
    Contents, Event, Object, Operation, PathBytes, decoding::Decoder, encoding::Encoder,
    prefixes::PrefixLoader,
};

#[derive(Clone, Copy)]
pub struct BenchmarkOptions {
    warmups: usize,
    duration: Duration,
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        Self {
            warmups: 10,
            duration: Duration::from_secs(5),
        }
    }
}

// implements welford's online algorithm
pub fn benchmark(
    func: impl Fn() -> Result<(), Failed>,
    options: BenchmarkOptions,
) -> impl FnOnce(bool) -> Result<Option<Measurement>, Failed> {
    move |first| {
        if first {
            func().map(|_| None)
        } else {
            for _ in 0..options.warmups {
                func()?;
            }

            let mut count: u64 = 0;
            let mut mean: f64 = 0.0;
            let mut m2: f64 = 0.0;

            let end = Instant::now() + options.duration;

            while Instant::now() < end || count == 0 {
                let t0 = Instant::now();
                func()?;
                let s = t0.elapsed().as_nanos() as f64;

                count += 1;

                let delta = s - mean;
                mean += delta / count as f64;

                let delta2 = s - mean;
                m2 += delta * delta2;
            }

            let variance = m2 / count as f64;
            let stddev = variance.sqrt();

            Ok(Some(Measurement {
                avg: mean.round() as u64,
                variance: stddev.round() as u64,
            }))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArbitraryLoader {
    chunks: [Bytes; 12],
    i: usize,
}

impl Arbitrary<'_> for ArbitraryLoader {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let chunks = <[&[u8]; 12]>::arbitrary(u)?.map(Bytes::copy_from_slice);
        Ok(Self { chunks, i: 0 })
    }
}

impl PrefixLoader for ArbitraryLoader {
    fn load(&mut self, _id: Hash) -> Result<Bytes, xh_archive::prefixes::Error> {
        let chunk = self.chunks[self.i % self.chunks.len()].clone();
        self.i += 1;
        Ok(chunk)
    }
}

#[derive(Debug, Clone)]
pub struct ArbitraryArchive {
    pub events: Vec<Event>,
}

impl Arbitrary<'_> for ArbitraryArchive {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let mut events = Vec::new();

        let index: BTreeSet<_> = u
            .arbitrary_iter()?
            .map(|data| {
                Ok(PathBytes {
                    inner: Bytes::copy_from_slice(data?),
                })
            })
            .collect::<Result<_, _>>()?;
        let objects = index.len();
        events.push(Event::Index(index));

        for _ in 0..objects {
            let operation = match u.choose_index(1)? {
                0 => Operation::Create {
                    permissions: *u.choose(&[0o755, 0o644])?,
                    object: {
                        match u.choose_index(2)? {
                            0 => Object::File {
                                prefix: u
                                    .arbitrary::<bool>()?
                                    .then_some(Hash::from_bytes([0; blake3::OUT_LEN])),
                                contents: Contents::Decompressed(
                                    u.arbitrary().map(Bytes::copy_from_slice)?,
                                ),
                            },
                            1 => Object::Symlink {
                                target: PathBytes {
                                    inner: u.arbitrary().map(Bytes::copy_from_slice)?,
                                },
                            },
                            2 => Object::Directory,
                            _ => unreachable!(),
                        }
                    },
                },
                1 => Operation::Delete,
                _ => unreachable!(),
            };

            events.push(Event::Operation(operation));
        }

        Ok(Self { events })
    }
}

pub fn compress(events: Vec<Event>, loader: impl PrefixLoader) -> Vec<Event> {
    Compressor::new()
        .with_loader(loader)
        .compress(events)
        .map(|event| event.expect("should be able to compress event"))
        .collect()
}

pub fn decompress(events: Vec<Event>, loader: impl PrefixLoader) -> Vec<Event> {
    Decompressor::new()
        .with_loader(loader)
        .decompress(events)
        .map(|event| event.expect("should be able to decompress event"))
        .collect()
}

pub fn decode(mut contents: &[u8]) -> Vec<Event> {
    let decoded = Decoder::new(&mut contents)
        .decode()
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail");

    #[cfg(feature = "log")]
    log::debug!("decoder output: {decoded:?}");

    decoded
}

pub fn encode(events: &Vec<Event>) -> Vec<u8> {
    let mut encoded = BytesMut::new();
    Encoder::new(&mut encoded)
        .encode(events)
        .expect("encoding should not fail");

    #[cfg(feature = "log")]
    log::debug!("encoder output: {encoded:?}");

    encoded.to_vec()
}

#[cfg(feature = "log")]
#[inline]
pub fn setup() {
    use fern::colors::{Color, ColoredLevelConfig};

    let colors = ColoredLevelConfig::new()
        .info(Color::Blue)
        .debug(Color::Magenta)
        .trace(Color::BrightBlack)
        .warn(Color::Yellow)
        .error(Color::Red);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(std::io::stderr())
        .apply()
        .expect("should be able to enable logger");
}

#[inline]
#[cfg(not(feature = "log"))]
pub fn setup() {}

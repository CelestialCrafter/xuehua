use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use arbitrary::Arbitrary;
use bytes::{Bytes, BytesMut};
use libtest_mimic::{Failed, Measurement};
use nix_archive::{Event, decoding::Decoder, encoding::Encoder};

pub fn decode(contents: &[u8]) -> Vec<Event> {
    let decoded = Decoder::new()
        .decode(&mut Bytes::copy_from_slice(contents))
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail");

    #[cfg(feature = "log")]
    log::debug!("decoder output: {decoded:?}");

    decoded
}

pub fn encode(events: &Vec<Event>) -> Vec<u8> {
    let mut encoded = BytesMut::new();
    Encoder::new()
        .encode(&mut encoded, events)
        .expect("encoding should not fail");

    #[cfg(feature = "log")]
    log::debug!("encoder output: {encoded:?}");

    encoded.to_vec()
}

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

// collapses multiple chunk events so comparing equality between
// semantically equivalent event streams doesn't error
pub fn chunk_collapse(events: Vec<Event>) -> Vec<Event> {
    let length = events.len();
    events
        .into_iter()
        .fold(Vec::with_capacity(length), |mut acc, mut event| {
            if let Event::RegularContentChunk(ref mut chunk) = event {
                acc.pop_if(|parent| match parent {
                    Event::RegularContentChunk(parent) => {
                        let mut bytes = BytesMut::new();
                        bytes.extend_from_slice(&parent);
                        bytes.extend_from_slice(&chunk);
                        *chunk = bytes.freeze();
                        true
                    }
                    _ => false,
                });
            }

            acc.push(event);
            acc
        })
}

#[derive(Debug)]
struct ArbitraryObject(Vec<Event>);

impl Arbitrary<'_> for ArbitraryObject {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let mut events = Vec::new();

        match u.choose_index(3)? {
            0 => {
                let mut size = u.arbitrary_len::<&[u8]>()?;
                events.push(Event::Regular {
                    executable: u.arbitrary()?,
                    size: size as u64,
                });

                if size == 0 {
                    events.push(Event::RegularContentChunk(Bytes::new()));
                }

                while size != 0 {
                    let chunk_size = u.int_in_range(1..=size)?;
                    size -= chunk_size;

                    let data = u.bytes(chunk_size)?;
                    events.push(Event::RegularContentChunk(Bytes::copy_from_slice(data)));
                }
            }
            1 => events.push(Event::Symlink {
                target: u.arbitrary().map(bytes::Bytes::copy_from_slice)?,
            }),
            2 => {
                events.push(Event::Directory);

                const MAX_FILES: u32 = 8;
                let mut prefix: usize = 0;
                u.arbitrary_loop(None, Some(MAX_FILES), |u| {
                    prefix += 1;

                    events.push(Event::DirectoryEntry {
                        name: [prefix.to_le_bytes(), u.arbitrary()?].concat().into(),
                    });
                    events.extend(Self::arbitrary(u)?.0);

                    Ok(ControlFlow::Continue(()))
                })?;

                events.push(Event::DirectoryEnd);
            }
            _ => unreachable!(),
        }

        Ok(Self(events))
    }
}

#[derive(Debug)]
pub struct ArbitraryNar {
    pub events: Vec<Event>,
}

impl Arbitrary<'_> for ArbitraryNar {
    #[inline]
    fn arbitrary(u: &mut arbitrary::Unstructured) -> Result<Self, arbitrary::Error> {
        let mut events = Vec::with_capacity(1);
        events.push(Event::Header);
        events.extend(ArbitraryObject::arbitrary(u)?.0);
        Ok(Self { events })
    }
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

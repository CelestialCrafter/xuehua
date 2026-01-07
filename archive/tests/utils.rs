#[cfg(feature = "std")]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arbitrary::Arbitrary;
use bytes::{Bytes, BytesMut};
use libtest_mimic::{Failed, Measurement};
use log::debug;
use xh_archive::{Event, Object, ObjectContent, decoding::Decoder, encoding::Encoder};

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
    func: impl Fn(),
    options: BenchmarkOptions,
) -> impl FnOnce(bool) -> Result<Option<Measurement>, Failed> {
    move |first| {
        if first {
            func();
            Ok(None)
        } else {
            for _ in 0..options.warmups {
                func();
            }

            let mut count: u64 = 0;
            let mut mean: f64 = 0.0;
            let mut m2: f64 = 0.0;

            let end = Instant::now() + options.duration;

            while Instant::now() < end || count == 0 {
                let t0 = Instant::now();
                func();
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
pub struct ArbitraryArchive {
    pub events: Vec<Event>,
}

impl Arbitrary<'_> for ArbitraryArchive {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let signatures = u
            .arbitrary_iter()?
            .map(|result| {
                result.map(|(hash, signature)| {
                    (
                        blake3::Hash::from_bytes(hash),
                        ed25519_dalek::Signature::from_bytes(&signature),
                    )
                })
            })
            .collect::<Result<_, _>>()?;

        let events = std::iter::once(Ok(Event::Header))
            .chain((0..u.arbitrary_len::<(&[u8], &[u8], u8, u32)>()?).map(|_| {
                let bytes = |u: &mut arbitrary::Unstructured<'_>| {
                    Ok(Bytes::from_owner(u.arbitrary::<Vec<u8>>()?))
                };

                let object = Object {
                    location: bytes(u)?.into(),
                    permissions: u.arbitrary()?,
                    content: match u.choose_index(3)? {
                        0 => ObjectContent::File { data: bytes(u)? },
                        1 => ObjectContent::Symlink {
                            target: bytes(u)?.into(),
                        },
                        2 => ObjectContent::Directory,
                        _ => unreachable!(),
                    },
                };

                Ok(Event::Object(object))
            }))
            .chain(std::iter::once(Ok(Event::Footer(signatures))))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { events })
    }
}

#[cfg(feature = "std")]
pub fn pack(root: &Path) -> Vec<Event> {
    xh_archive::packing::Packer::new(root.to_path_buf())
        .pack_iter()
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn pack_mmap(root: &Path) -> Vec<Event> {
    let mut packer = xh_archive::packing::Packer::new(root.to_path_buf());
    unsafe { packer.pack_mmap_iter() }
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(feature = "std")]
pub fn unpack(root: &Path, events: &Vec<Event>) {
    xh_archive::unpacking::Unpacker::new(root)
        .unpack_iter(events)
        .expect("should be able to unpack files")
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn unpack_mmap(root: &Path, events: &Vec<Event>) {
    let mut unpacker = xh_archive::unpacking::Unpacker::new(root);
    unsafe { unpacker.unpack_mmap_iter(events) }.expect("should be able to unpack files")
}

pub fn decode(contents: &mut Bytes) -> Vec<Event> {
    let decoded = Decoder::new()
        .decode_iter(contents)
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail");

    debug!("decoded events: {decoded:?}");
    decoded
}

pub fn encode(events: &Vec<Event>) -> Bytes {
    let mut encoded = BytesMut::new();
    Encoder::new().encode_iter(&mut encoded, events);

    debug!("encoded data: {encoded:?}");
    encoded.freeze()
}

#[cfg(feature = "std")]
pub fn make_temp() -> (PathBuf, tempfile::TempDir) {
    let temp =
        tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("should be able to make temp dir");
    let path = temp.path().to_path_buf();

    (path, temp)
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

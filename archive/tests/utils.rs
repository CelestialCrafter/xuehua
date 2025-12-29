#[cfg(feature = "std")]
use std::path::Path;
use std::time::{Duration, Instant};

use arbitrary::Arbitrary;
use bytes::{Bytes, BytesMut};
use libtest_mimic::{Failed, Measurement};
use log::debug;
use xh_archive::{Object, ObjectContent, decoding::Decoder, encoding::Encoder};

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
    pub objects: Vec<Object>,
}

impl Arbitrary<'_> for ArbitraryArchive {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let objects = (0..u.arbitrary_len::<(&[u8], &[u8], u8, u32)>()?)
            .map(|_| {
                let bytes = |u: &mut arbitrary::Unstructured<'_>| {
                    Ok(Bytes::from_owner(u.arbitrary::<Vec<u8>>()?))
                };

                Ok(Object {
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
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { objects })
    }
}

#[cfg(feature = "std")]
pub fn pack(root: &Path) -> Vec<Object> {
    xh_archive::packing::Packer::new(root.to_path_buf())
        .pack()
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn pack_mmap(root: &Path) -> Vec<Object> {
    let mut packer = xh_archive::packing::Packer::new(root.to_path_buf());
    unsafe { packer.pack_mmap() }
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(feature = "std")]
pub fn unpack(root: &Path, events: &Vec<Object>) {
    xh_archive::unpacking::Unpacker::new(root)
        .unpack(events)
        .expect("should be able to unpack files")
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn unpack_mmap(root: &Path, events: &Vec<Object>) {
    let mut unpacker = xh_archive::unpacking::Unpacker::new(root);
    unsafe { unpacker.unpack_mmap(events) }.expect("should be able to unpack files")
}

pub fn decode(contents: &mut Bytes) -> Vec<Object> {
    Decoder::new()
        .decode(contents)
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail")
}

#[cfg(feature = "std")]
pub fn decode_reader(contents: &mut impl std::io::Read) -> Vec<Object> {
    let decoded = Decoder::new()
        .decode_reader(contents)
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail");

    debug!("decoded events: {decoded:?}");
    decoded
}

pub fn encode(events: &Vec<Object>) -> Bytes {
    let mut encoded = BytesMut::new();
    Encoder::new().encode(&mut encoded, events);

    debug!("encoded data: {encoded:?}");
    encoded.freeze()
}

#[cfg(feature = "std")]
pub fn encode_writer(events: &Vec<Object>, writer: &mut impl std::io::Write) {
    Encoder::new()
        .encode_writer(writer, events)
        .expect("encoding should not fail");
}

#[cfg(feature = "std")]
 pub fn make_temp() -> (std::path::PathBuf, tempfile::TempDir) {
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

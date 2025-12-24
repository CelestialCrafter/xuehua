use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arbitrary::Arbitrary;
use bytes::{Bytes, BytesMut};
use libtest_mimic::{Failed, Measurement};
use xh_archive::{
    Event, Index, Object, ObjectMetadata, ObjectType, PathBytes, decoding::Decoder,
    encoding::Encoder,
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
pub struct ArbitraryArchive {
    pub events: Vec<Event>,
}

impl Arbitrary<'_> for ArbitraryArchive {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        fn arbitrary_bytes(u: &mut arbitrary::Unstructured<'_>) -> Result<Bytes, arbitrary::Error> {
            Vec::<u8>::arbitrary(u).map(Into::into)
        }

        let index: Index = (0..u.arbitrary_len::<(u8, &[u8], &[u8], u32)>()?)
            .map(|_| {
                let variant = match u.choose_index(2)? {
                    0 => ObjectType::File,
                    1 => ObjectType::Symlink,
                    2 => ObjectType::Directory,
                    _ => unreachable!()
                };

                let size = if let ObjectType::Directory = variant {
                    0
                } else {
                    u.arbitrary_len::<&[u8]>()? as u64
                };

                Ok((
                    PathBytes {
                        inner: arbitrary_bytes(u)?,
                    },
                    ObjectMetadata {
                        permissions: u.arbitrary()?,
                        size,
                        variant,
                    },
                ))
            })
            .collect::<Result<_, _>>()?;

        let objects = index
            .iter()
            .map(|(_, metadata)| {
                let mut content = || {
                    let mut content = BytesMut::zeroed(metadata.size as usize);
                    u.fill_buffer(&mut content)?;
                    Ok(content.freeze())
                };

                Ok(Event::Object(match metadata.variant {
                    ObjectType::File => Object::File {
                        contents: content()?,
                    },
                    ObjectType::Symlink => Object::Symlink {
                        target: content()?.into(),
                    },
                    ObjectType::Directory => Object::Directory,
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut events = vec![Event::Index(index)];
        events.extend(objects);

        Ok(Self { events })
    }
}

#[cfg(feature = "std")]
pub fn pack(root: &Path) -> Vec<Event> {
    xh_archive::packing::Packer::new(root.to_path_buf())
        .pack()
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn pack_mmap(root: &Path) -> Vec<Event> {
    let mut packer = xh_archive::packing::Packer::new(root.to_path_buf());
    unsafe { packer.pack_mmap() }
        .map(|event| event.expect("should be able to pack file"))
        .collect()
}

#[cfg(feature = "std")]
pub fn unpack(root: &Path, events: &Vec<Event>) {
    xh_archive::unpacking::Unpacker::new(root)
        .unpack(events)
        .expect("should be able to unpack files")
}

#[cfg(all(feature = "std", feature = "mmap"))]
pub fn unpack_mmap(root: &Path, events: &Vec<Event>) {
    let mut unpacker = xh_archive::unpacking::Unpacker::new(root);
    unsafe { unpacker.unpack_mmap(events) }.expect("should be able to unpack files")
}

pub fn decode(mut contents: &[u8]) -> Vec<Event> {
    let mut decoder = Decoder::new(&mut contents);

    let decoded = decoder
        .decode()
        .collect::<Result<Vec<_>, _>>()
        .expect("decoding should not fail");
    assert!(decoder.finished(), "decoding should be finished");

    decoded
}

pub fn encode(events: &Vec<Event>) -> Vec<u8> {
    let mut encoded = BytesMut::new();
    let mut encoder = Encoder::new(&mut encoded);

    encoder.encode(events).expect("encoding should not fail");
    assert!(encoder.finished(), "encoding should be finished");

    encoded.to_vec()
}

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

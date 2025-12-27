use std::ffi::OsStr;

use arbitrary::Arbitrary;
use arbtest::arbtest;
use bytes::Bytes;
use include_dir::include_dir;
use libtest_mimic::{Arguments, Trial};
use xh_archive::Event;

use crate::utils::{ArbitraryArchive, BenchmarkOptions, benchmark, decode, encode, setup};

mod utils;

#[cfg(feature = "std")]
#[inline]
fn pack_unpack_roundtrip(events: &Vec<Event>) {
    let (path, _temp) = utils::make_temp();

    utils::unpack(&path, events);
    assert_eq!(events, &utils::pack(&path));
}

#[cfg(all(feature = "std", feature = "mmap"))]
#[inline]
fn mmap_pack_unpack_roundtrip(events: &Vec<Event>) {
    let (path, _temp) = utils::make_temp();

    utils::unpack_mmap(&path, events);
    assert_eq!(events, &utils::pack_mmap(&path));
}

#[inline]
fn enc_dec_roundtrip(events: &Vec<Event>) {
    assert_eq!(events, &decode(&mut encode(events)));
}

#[cfg(feature = "std")]
#[inline]
fn io_enc_dec_roundtrip(events: &Vec<Event>) {
    use bytes::{Buf, BufMut, BytesMut};

    let mut writer = BytesMut::new().writer();
    utils::encode_writer(events, &mut writer);

    let mut reader = writer.into_inner().reader();
    assert_eq!(events, &utils::decode_reader(&mut reader));
}

#[inline]
fn arbitrary_trials() -> impl Iterator<Item = Trial> {
    fn trial<F>(name: &str, runner: F) -> Trial
    where
        F: Fn(&Vec<Event>),
        F: Send + Sync + 'static,
    {
        Trial::test(name, move || {
            arbtest(|u| {
                runner(&ArbitraryArchive::arbitrary(u)?.events);
                Ok(())
            })
            .run();
            Ok(())
        })
    }

    [
        trial("enc-dec-arbitrary", enc_dec_roundtrip),
        #[cfg(feature = "std")]
        trial("io-enc-dec-arbitrary", io_enc_dec_roundtrip),
    ]
    .into_iter()
    .map(|trial| trial.with_kind("arbitrary"))
}

#[inline]
fn blob_trials() -> impl Iterator<Item = Trial> {
    let trials = |name, mut contents| {
        let events = Box::leak(Box::new(decode(&mut contents)));
        let options = BenchmarkOptions::default();

        [
            Trial::bench(
                format!("enc-dec-{name}"),
                benchmark(|| enc_dec_roundtrip(events), options),
            ),
            #[cfg(feature = "std")]
            Trial::bench(
                format!("io-enc-dec-{name}"),
                benchmark(|| io_enc_dec_roundtrip(events), options),
            ),
            #[cfg(feature = "std")]
            Trial::bench(
                format!("pack-unpack-{name}"),
                benchmark(|| pack_unpack_roundtrip(events), options),
            ),
            #[cfg(all(feature = "std", feature = "mmap"))]
            Trial::bench(
                format!("mmap-pack-unpack-{name}"),
                benchmark(|| Ok(mmap_pack_unpack_roundtrip(events)), options),
            ),
        ]
    };

    include_dir!("$CARGO_MANIFEST_DIR/tests/blobs")
        .files()
        .filter(|file| file.path().extension() == Some(OsStr::new("xhar")))
        .map(move |file| {
            trials(
                file.path().file_stem().unwrap().to_string_lossy(),
                Bytes::copy_from_slice(file.contents()),
            )
        })
        .flatten()
        .map(|trial| trial.with_kind("blob"))
}

fn main() {
    let trials = blob_trials().chain(arbitrary_trials()).collect();
    setup();

    libtest_mimic::run(&Arguments::from_args(), trials).exit()
}

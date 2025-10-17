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
fn pack_unpack_roundtrip(events: &Vec<Event>, assert: bool) {
    let (path, _temp) = utils::make_temp();

    utils::unpack(&path, events);
    if assert {
        assert_eq!(events, &utils::pack(&path));
    }
}

#[cfg(all(feature = "std", feature = "mmap"))]
fn mmap_pack_unpack_roundtrip(events: &Vec<Event>, assert: bool) {
    let (path, _temp) = utils::make_temp();

    utils::unpack_mmap(&path, events);
    if assert {
        assert_eq!(events, &utils::pack_mmap(&path));
    }
}

fn enc_dec_roundtrip(events: &Vec<Event>, assert: bool) {
    let decoded = decode(&mut encode(events));
    if assert {
        assert_eq!(events, &decoded);
    }
}

fn arbitrary_trials() -> impl Iterator<Item = Trial> {
    fn trial<F>(name: &str, runner: F) -> Trial
    where
        F: Fn(&Vec<Event>, bool),
        F: Send + Sync + 'static,
    {
        Trial::test(name, move || {
            arbtest(|u| {
                // arbitrary tests arent used for benchmarks
                runner(&ArbitraryArchive::arbitrary(u)?.events, true);
                Ok(())
            })
            .run();
            Ok(())
        })
    }

    [trial("enc-dec-arbitrary", enc_dec_roundtrip)]
        .into_iter()
        .map(|trial| trial.with_kind("arbitrary"))
}

fn blob_trials() -> impl Iterator<Item = Trial> {
    let trials = |name, mut contents| {
        let events = Box::leak(Box::new(decode(&mut contents)));
        let options = BenchmarkOptions::default();

        [
            Trial::bench(
                format!("enc-dec-{name}"),
                benchmark(|| enc_dec_roundtrip(events, false), options),
            ),
            #[cfg(feature = "std")]
            Trial::bench(
                format!("pack-unpack-{name}"),
                benchmark(|| pack_unpack_roundtrip(events, false), options),
            ),
            #[cfg(all(feature = "std", feature = "mmap"))]
            Trial::bench(
                format!("mmap-pack-unpack-{name}"),
                benchmark(|| mmap_pack_unpack_roundtrip(events, false), options),
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

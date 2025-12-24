use std::ffi::OsStr;

use arbitrary::Arbitrary;
use arbtest::arbtest;
use include_dir::include_dir;
use libtest_mimic::{Arguments, Trial};
use xh_archive::Event;

use crate::utils::{
    ArbitraryArchive, BenchmarkOptions, benchmark, decode, encode, make_temp, setup,
};

mod utils;

#[cfg(feature = "std")]
#[inline]
fn pack_unpack_roundtrip(events: Vec<Event>) {
    let (path, _temp) = make_temp();

    utils::unpack(&path, &events);
    assert_eq!(events, utils::pack(&path));
}

#[cfg(all(feature = "std", feature = "mmap"))]
#[inline]
fn mmap_pack_unpack_roundtrip(events: Vec<Event>) {
    let (path, _temp) = make_temp();

    utils::unpack_mmap(&path, &events);
    assert_eq!(events, utils::pack_mmap(&path));
}

#[inline]
fn enc_dec_roundtrip(events: Vec<Event>) {
    assert_eq!(events, decode(&encode(&events)));
}

#[inline]
fn arbitrary_trials() -> impl Iterator<Item = Trial> {
    fn trial<F>(name: &str, runner: F) -> Trial
    where
        F: Fn(&mut arbitrary::Unstructured<'_>) -> Result<(), arbitrary::Error>,
        F: Send,
        F: 'static,
    {
        Trial::test(name, || {
            arbtest(runner).run();
            Ok(())
        })
    }

    [trial("enc-dec-arbitrary", |u| {
        enc_dec_roundtrip(ArbitraryArchive::arbitrary(u)?.events);
        Ok(())
    })]
    .into_iter()
    .map(|trial| trial.with_kind("arbitrary"))
}

#[inline]
fn blob_trials() -> impl Iterator<Item = Trial> {
    let trials = |name, contents| {
        let events = Box::leak(Box::new(decode(contents)));
        let options = BenchmarkOptions::default();

        [
            Trial::bench(
                format!("enc-dec-{name}"),
                benchmark(|| Ok(enc_dec_roundtrip(events.to_vec())), options),
            ),
            #[cfg(feature = "std")]
            Trial::bench(
                format!("pack-unpack-{name}"),
                benchmark(|| Ok(pack_unpack_roundtrip(events.to_vec())), options),
            ),
            #[cfg(all(feature = "std", feature = "mmap"))]
            Trial::bench(
                format!("mmap-pack-unpack-{name}"),
                benchmark(|| Ok(mmap_pack_unpack_roundtrip(events.to_vec())), options),
            ),
        ]
    };

    include_dir!("$CARGO_MANIFEST_DIR/tests/blobs")
        .files()
        .filter(|file| file.path().extension() == Some(OsStr::new("xhar")))
        .map(move |file| {
            trials(
                file.path().file_stem().unwrap().to_string_lossy(),
                file.contents(),
            )
        })
        .flatten()
        .map(|trial| trial.with_kind("blob"))
}

fn main() {
    setup();

    let trials = blob_trials().chain(arbitrary_trials()).collect();
    libtest_mimic::run(&Arguments::from_args(), trials).exit()
}

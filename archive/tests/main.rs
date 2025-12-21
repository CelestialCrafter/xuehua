use std::ffi::OsStr;

use arbitrary::Arbitrary;
use arbtest::arbtest;
use include_dir::include_dir;
use libtest_mimic::{Arguments, Trial};
use xh_archive::{
    Event,
    prefixes::{PrefixLoader, unimplemented::UnimplementedLoader},
};

use crate::utils::{
    ArbitraryArchive, ArbitraryLoader, BenchmarkOptions, benchmark, compress, decode, decompress,
    encode, pack, setup, unpack,
};

mod utils;

#[inline]
fn pack_unpack_roundtrip(events: Vec<Event>) {
    let events = decompress(events, UnimplementedLoader);

    let temp =
        tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("should be able to make temp dir");
    let path = temp.path().join("root");

    unpack(&path, &events);
    assert_eq!(events, pack(&path));
}

#[inline]
fn comp_decomp_roundtrip(events: Vec<Event>, loader: impl PrefixLoader + Clone) {
    assert_eq!(
        events,
        compress(decompress(events.clone(), loader.clone()), loader)
    )
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

    fn events(
        u: &mut arbitrary::Unstructured<'_>,
    ) -> Result<(Vec<Event>, ArbitraryLoader), arbitrary::Error> {
        let loader = ArbitraryLoader::arbitrary(u)?;
        let events = compress(ArbitraryArchive::arbitrary(u)?.events, loader.clone());

        Ok((events, loader))
    }

    [
        trial("comp-decomp-arbitrary", |u| {
            let (events, loader) = events(u)?;
            comp_decomp_roundtrip(events, loader);
            Ok(())
        }),
        trial("enc-dec-arbitrary", |u| {
            let (events, _) = events(u)?;
            enc_dec_roundtrip(events);
            Ok(())
        }),
    ]
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
            Trial::bench(
                format!("comp-decomp-{name}"),
                benchmark(
                    || Ok(comp_decomp_roundtrip(events.to_vec(), UnimplementedLoader)),
                    options,
                ),
            ),
            Trial::bench(
                format!("pack-unpack-{name}"),
                benchmark(|| Ok(pack_unpack_roundtrip(events.to_vec())), options),
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

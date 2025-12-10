use std::ffi::OsStr;

use arbitrary::Arbitrary;
use arbtest::arbtest;
use include_dir::include_dir;
use libtest_mimic::{Arguments, Trial};
use xh_archive::prefixes::unimplemented::UnimplementedLoader;

use crate::utils::{
    ArbitraryArchive, ArbitraryLoader, BenchmarkOptions, benchmark, compress, decode, decompress,
    encode, setup,
};

mod utils;

#[inline]
fn comp_decomp_roundtrip(contents: &[u8]) {
    let events = decode(contents);
    assert_eq!(
        events,
        compress(
            decompress(events.clone(), UnimplementedLoader),
            UnimplementedLoader
        )
    )
}

#[inline]
fn enc_dec_roundtrip(contents: &[u8]) {
    assert_eq!(contents, encode(&decode(contents)));
}

#[inline]
fn arbitrary_trials() -> impl Iterator<Item = Trial> {
    [
        Trial::test("arbitrary", || {
            arbtest(|u| {
                let events = ArbitraryArchive::arbitrary(u)?.events;
                let events = compress(events, ArbitraryLoader::arbitrary(u)?);

                assert_eq!(events, decode(&encode(&events)));

                Ok(())
            })
            .run();

            Ok(())
        })
        .with_kind("enc-dec"),
        Trial::test("arbitrary", || {
            arbtest(|u| {
                let events = ArbitraryArchive::arbitrary(u)?.events;
                let loader = ArbitraryLoader::arbitrary(u)?;

                assert_eq!(
                    events,
                    decompress(compress(events.clone(), loader.clone()), loader.clone())
                );

                Ok(())
            })
            .run();

            Ok(())
        })
        .with_kind("comp-decomp"),
    ]
    .into_iter()
}

#[inline]
fn blob_trials() -> impl Iterator<Item = Trial> {
    include_dir!("$CARGO_MANIFEST_DIR/tests/blobs")
        .files()
        .filter(|file| file.path().extension() == Some(OsStr::new("xhar")))
        .map(move |file| {
            let contents = std::hint::black_box(file.contents());
            let name = file.path().file_stem().unwrap().to_string_lossy();

            [
                Trial::bench(
                    name.clone(),
                    benchmark(
                        || Ok(enc_dec_roundtrip(contents)),
                        BenchmarkOptions::default(),
                    ),
                )
                .with_kind("enc-dec"),
                Trial::bench(
                    name,
                    benchmark(
                        || Ok(comp_decomp_roundtrip(contents)),
                        BenchmarkOptions::default(),
                    ),
                )
                .with_kind("comp-decomp"),
            ]
        })
        .flatten()
}

fn main() {
    setup();

    let trials = blob_trials().chain(arbitrary_trials()).collect();
    libtest_mimic::run(&Arguments::from_args(), trials).exit()
}

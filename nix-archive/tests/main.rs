use std::{ffi::OsStr, iter::once};

use arbitrary::Arbitrary;
use arbtest::arbtest;
use include_dir::include_dir;
use libtest_mimic::{Arguments, Failed, Trial};

use crate::utils::{
    ArbitraryNar, BenchmarkOptions, benchmark, chunk_collapse, decode, encode, setup,
};

mod utils;

#[inline]
fn coding_roundtrip(contents: &[u8]) -> Result<(), Failed> {
    assert_eq!(contents, encode(&decode(contents)));
    Ok(())
}

#[inline]
fn arbitrary_trials() -> impl Iterator<Item = Trial> {
    once(
        Trial::test("arbitrary", || {
            arbtest(|u| {
                let events = chunk_collapse(ArbitraryNar::arbitrary(u)?.events);
                assert_eq!(events, chunk_collapse(decode(&encode(&events))));

                Ok(())
            })
            .run();

            Ok(())
        })
        .with_kind("coding"),
    )
}

#[inline]
fn blob_trials() -> impl Iterator<Item = Trial> {
    include_dir!("$CARGO_MANIFEST_DIR/tests/blobs")
        .files()
        .filter(|file| file.path().extension() == Some(OsStr::new("nar")))
        .map(move |file| {
            let contents = std::hint::black_box(file.contents());
            let name = file.path().file_stem().unwrap().to_string_lossy();

            Trial::bench(
                name,
                benchmark(|| coding_roundtrip(contents), BenchmarkOptions::default()),
            )
            .with_kind("coding")
        })
}

fn main() {
    setup();

    let trials = blob_trials().chain(arbitrary_trials()).collect();
    libtest_mimic::run(&Arguments::from_args(), trials).exit()
}

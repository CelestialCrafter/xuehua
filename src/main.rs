pub mod options;
pub mod package;
pub mod store;

use std::{fs, path::Path, sync::LazyLock};

use blake3::Hash;

use crate::{
    options::OPTIONS,
    package::build::build,
    store::fetcher::{self, FetchOptions},
};

fn main() {
    LazyLock::force(&OPTIONS);
    println!("{:?}", OPTIONS);

    println!(
        "{:?}",
        build(fs::read("./package.lua").expect("could not open package.lua"))
    );

    println!(
        "{:?}",
        fetcher::fetch(FetchOptions {
            url: "https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/aarch64/alpine-minirootfs-3.22.1-aarch64.tar.gz",
            hash: Hash::from_hex("40eb8714729db02cb26741d15c302c0e9f610142770715f1b7479183f4da88d9").unwrap(),
            store: Path::new("./"),
            curl_opts: &[]
        })
    )
}

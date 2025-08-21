pub mod options;
pub mod package;

use std::{fs, sync::LazyLock};

use crate::{options::OPTIONS, package::build::build};

fn main() {
    LazyLock::force(&OPTIONS);
    println!("{:?}", OPTIONS);

    println!(
        "{:?}",
        build(fs::read("./package.lua").expect("could not open package.lua"))
    );
}

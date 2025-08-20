pub mod options;
pub mod package;

use std::fs;

use crate::{options::options, package::build};

fn main() {
    println!("{:?}", options().run());

    println!(
        "{:?}",
        build(fs::read("./package.lua").expect("could not open package.lua"))
    );
}

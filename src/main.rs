pub mod options;
pub mod package;
pub mod store;

use std::fs;

use crate::options::OPTIONS;
use crate::options::cli::Subcommand;
use crate::package::build::build;

fn main() {
    match &OPTIONS.cli.subcommand {
        Subcommand::Build { package } => {
            eprintln!("building {package}");
            build(fs::read("xuehua/main.lua").expect("could not open package.lua"))
                .expect("could not build package");
        }
        Subcommand::Link {
            reverse: _,
            package: _,
        } => todo!("link not yet implemented"),
        Subcommand::Shell { package: _ } => todo!("shell not yet implemented"),
        Subcommand::GC => todo!("gc not yet implemented"),
        Subcommand::Repair => todo!("repair not yet implemented"),
    }
}

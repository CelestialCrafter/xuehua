pub mod fetcher;
pub mod options;
pub mod evaluator;
pub mod package;

use std::path::Path;

use crate::options::OPTIONS;
use crate::options::cli::Subcommand;
use crate::package::namespace::Resolver;

fn main() {
    match &OPTIONS.cli.subcommand {
        Subcommand::Build { package } => {
            let mut resolver = Resolver::new(Path::new("xuehua/main.lua"));
            let package = resolver.find(package);
            println!("{:?}", package);
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

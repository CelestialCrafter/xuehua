pub mod fetcher;
pub mod options;
pub mod pkgsys;

use std::path::PathBuf;

use crate::options::OPTIONS;
use crate::options::cli::Subcommand;
use crate::pkgsys::namespace::NamespaceResolver;

fn main() {
    match &OPTIONS.cli.subcommand {
        Subcommand::Build { package } => {
            let mut resolver = NamespaceResolver::new(PathBuf::from("xuehua/main.lua"));
            let package = resolver.resolve(package, resolver.root);
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

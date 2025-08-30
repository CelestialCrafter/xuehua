pub mod options;
pub mod package;
pub mod error;
pub mod store;

use crate::options::OPTIONS;
use crate::options::cli::Subcommand;

fn main() {
    match &OPTIONS.cli.subcommand {
        Subcommand::Build { package: _ } => {}
        Subcommand::Link {
            reverse: _,
            package: _,
        } => todo!("link not yet implemented"),
        Subcommand::Shell { package: _ } => todo!("shell not yet implemented"),
        Subcommand::GC => todo!("gc not yet implemented"),
        Subcommand::Repair => todo!("repair not yet implemented"),
    }
}

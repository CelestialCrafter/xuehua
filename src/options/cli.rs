use crate::package::Id;

use clap::Parser;

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Subcommand {
    /// link a package and its dependencies
    Link {
        /// un-link a previously linked package
        #[arg(long, short)]
        reverse: bool,

        #[arg(value_name = "PACKAGE")]
        package: Id,
    },

    /// builds a package
    Build {
        #[arg(value_name = "PACKAGE")]
        package: Id,
    },

    /// start a shell in a package's environment
    Shell {
        #[arg(value_name = "PACKAGE")]
        package: Id,
    },

    /// remove unlinked packages from the store
    GC,

    /// verifies store contents, and re-links partially linked packages
    Repair,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliOptions {
    /// don't make any changes to the system
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[cfg(test)]
mod test {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn test_verify_args() {
        CliOptions::command().debug_assert();
    }
}

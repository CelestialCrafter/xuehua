use std::path::PathBuf;

use bpaf::{OptionParser, Parser, construct, long, positional, pure};
use humantime::Duration;

use crate::PackageId;

pub const DISPLAY_NAME: &str = "Xuěhuā";

#[derive(Debug, Clone)]
pub enum Subcommand {
    Link { id: PackageId, reverse: bool },
    Shell { id: PackageId },
    GC { older_than: Option<Duration> },
    Repair,
}

fn repair() -> impl Parser<Subcommand> {
    pure(Subcommand::Repair)
}

fn gc() -> impl Parser<Subcommand> {
    let older_than = long("older-than")
        .help("only remove packages older than x")
        .argument("DURATION")
        .optional();

    construct!(Subcommand::GC { older_than })
}

fn link() -> impl Parser<Subcommand> {
    let id = positional("PACKAGE").help("package to link/unlink");
    let reverse = long("reverse")
        .short('r')
        .help("un-link a previously linked package")
        .switch();

    construct!(Subcommand::Link { reverse, id })
}

#[derive(Debug)]
pub struct Options {
    pub subcommand: Subcommand,
    pub root: PathBuf,
    pub dry_run: bool,
}

pub fn options() -> OptionParser<Options> {
    let subcommand = {
        let link = link()
            .to_options()
            .descr("link a package and its dependencies")
            .command("link");
        let gc = gc()
            .to_options()
            .descr("remove unlinked packages from the store")
            .command("gc");
        let repair = repair()
            .to_options()
            .descr("verifies store contents, and re-links partially linked packages")
            .command("repair");

        construct!([link, gc, repair])
    };

    let dry_run = long("dry-run")
        .help("do not make any changes to the system")
        .switch();
    let root = long("root")
        .help(format!("root dierctory for {DISPLAY_NAME} to run in").as_str())
        .argument("DIRECTORY")
        .optional()
        .map(|root| root.unwrap_or("/".into()));

    construct!(Options {
        dry_run,
        root,
        subcommand
    })
    .to_options()
}

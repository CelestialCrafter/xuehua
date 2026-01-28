use std::{env, fmt, path::PathBuf, str::FromStr};

use bpaf::{OptionParser, Parser, construct, long, positional, pure};

use xh_engine::name::PackageName;

#[derive(Debug, Clone, Copy)]
pub struct FormatParseError;

impl fmt::Display for FormatParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not parse format")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProjectFormat {
    Dot,
    Json,
}

impl FromStr for ProjectFormat {
    type Err = FormatParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dot" => Ok(Self::Dot),
            "json" => Ok(Self::Json),
            _ => Err(FormatParseError),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PackageFormat {
    Human,
    Json,
}

impl FromStr for PackageFormat {
    type Err = FormatParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            _ => Err(FormatParseError),
        }
    }
}

#[derive(Debug, Clone)]
pub enum InspectAction {
    Project {
        format: ProjectFormat,
    },
    Packages {
        packages: Vec<PackageName>,
        format: PackageFormat,
    },
}

impl InspectAction {
    pub fn parser() -> impl Parser<Self> {
        let project = {
            let format = long("format")
                .short('f')
                .help("Project output format")
                .argument("FORMAT")
                .fallback(ProjectFormat::Dot);

            construct!(Self::Project { format })
                .to_options()
                .descr("Inspects the given project")
                .command("project")
        };

        let packages = {
            let packages = PackageAction::pkgs_parser();
            let format = long("format")
                .short('f')
                .help("Package output format")
                .argument("FORMAT")
                .fallback(PackageFormat::Human);

            construct!(Self::Packages { format, packages })
                .to_options()
                .descr("Inspects the given packages declarations")
                .command("packages")
        };

        construct!([project, packages])
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LinkAction {
    Add,
    Delete,
}

impl LinkAction {
    pub fn parser() -> impl Parser<Self> {
        let add = pure(Self::Add)
            .to_options()
            .descr("Add a package to the system's links")
            .command("add");
        let delete = pure(Self::Delete)
            .to_options()
            .descr("Delete a package from the system's links")
            .command("delete");

        construct!([add, delete])
    }
}

#[derive(Debug, Clone)]
pub enum PackageAction {
    Link {
        dry_run: bool,
        root: PathBuf,
        action: LinkAction,
        packages: Vec<PackageName>,
    },
    Build {
        dry_run: bool,
        packages: Vec<PackageName>,
    },
    Inspect(InspectAction),
}

impl PackageAction {
    fn pkgs_parser() -> impl Parser<Vec<PackageName>> {
        positional("PACKAGE").many()
    }

    fn parser() -> impl Parser<Self> {
        let dry_run = || {
            long("dry-run")
                .help("Run without making changes to the system")
                .switch()
        };

        let link = {
            let action = LinkAction::parser();
            let packages = Self::pkgs_parser();
            let root = long("root")
                .short('r')
                .help("Root filesystem to operate on")
                .argument("ROOT");

            construct!(Self::Link {
                root,
                dry_run(),
                action,
                packages
            })
            .to_options()
            .descr("Manage system links")
            .command("link")
        };

        let build = {
            let packages = Self::pkgs_parser();
            construct!(Self::Build { dry_run(), packages })
                .to_options()
                .descr("Builds packages")
                .command("build")
        };

        let inspect = {
            let action = InspectAction::parser();

            construct!(Self::Inspect(action))
                .to_options()
                .descr("Inspect a package or project")
                .command("inspect")
        };

        construct!([link, build, inspect])
    }
}

#[derive(Debug, Clone)]
pub enum ArchiveAction {
    Pack { path: PathBuf },
    Unpack { path: PathBuf },
    Decode,
    Hash,
}

impl ArchiveAction {
    fn path_parser() -> impl Parser<PathBuf> {
        long("path")
            .short('p')
            .help("Path to the archive or directory")
            .argument("PATH")
            .fallback_with(env::current_dir)
    }

    fn parser() -> impl Parser<Self> {
        let pack = {
            let path = Self::path_parser();
            construct!(Self::Pack { path })
                .to_options()
                .descr("Pack a directory into an archive")
                .command("pack")
        };

        let unpack = {
            let path = Self::path_parser();
            construct!(Self::Unpack { path })
                .to_options()
                .descr("Unpack an archive into a directory")
                .command("unpack")
        };

        // TODO: support json format
        let decode = pure(Self::Decode)
            .to_options()
            .descr("Decode an archive into events")
            .command("decode");

        let hash = pure(Self::Hash)
            .to_options()
            .descr("Hash an archive")
            .command("hash");

        construct!([pack, unpack, decode, hash])
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Package {
        project: PathBuf,
        action: PackageAction,
    },
    Archive(ArchiveAction),
}

impl Action {
    pub fn parser() -> impl Parser<Self> {
        let package = {
            let action = PackageAction::parser();
            let project = long("project")
                .short('p')
                .help("Path to the target project")
                .argument("PROJECT")
                .fallback_with(env::current_dir);

            construct!(Self::Package { project, action })
                .to_options()
                .descr("Manage packages")
                .command("package")
        };

        let archive = {
            let action = ArchiveAction::parser();

            construct!(Self::Archive(action))
                .to_options()
                .descr("Manage archives")
                .command("archive")
        };

        construct!([package, archive])
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub action: Action,
}

impl Options {
    pub fn options() -> OptionParser<Self> {
        let action = Action::parser();
        construct!(Self { action })
            .to_options()
            .fallback_to_usage()
            .version(env!("CARGO_PKG_VERSION"))
    }
}

mod tests {
    #[test]
    fn check_options() {
        super::Options::options().check_invariants(false)
    }
}

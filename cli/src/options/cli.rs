use std::{env, fmt, path::PathBuf, str::FromStr};

use bpaf::{OptionParser, Parser, construct, long, positional, pure};

use xh_engine::package::PackageId;

fn pkgs_parser() -> impl Parser<Vec<PackageId>> {
    positional("PACKAGE").many()
}

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

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
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

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
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
        packages: Vec<PackageId>,
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
            let packages = pkgs_parser();
            let format = long("format")
                .short('f')
                .help("Package output format")
                .argument("FORMAT")
                .fallback(PackageFormat::Human);

            construct!(Self::Packages { format, packages })
                .to_options()
                .descr("Inspects the given packages declarations")
                .command("package")
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
pub enum Action {
    Link {
        dry_run: bool,
        root: PathBuf,
        packages: Vec<PackageId>,
        action: LinkAction,
    },
    Build {
        dry_run: bool,
        packages: Vec<PackageId>,
    },
    Inspect(InspectAction),
}

impl Action {
    pub fn parser() -> impl Parser<Self> {
        let dry_run = || {
            long("dry-run")
                .help("Run without making changes to the system")
                .switch()
        };

        let link = {
            let action = LinkAction::parser();
            let packages = pkgs_parser();
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
            let packages = pkgs_parser();
            let dry_run = dry_run();

            construct!(Self::Build { dry_run, packages })
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
pub struct Options {
    pub project: PathBuf,
    pub action: Action,
}

impl Options {
    pub fn options() -> OptionParser<Self> {
        let action = Action::parser();

        let project = long("project")
            .short('p')
            .help("Path to the target project")
            .argument("PROJECT")
            .fallback_with(env::current_dir);

        construct!(Self { project, action })
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

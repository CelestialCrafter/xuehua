use std::{collections::VecDeque, env, fmt, path::PathBuf};

use pico_args::Arguments;
use smol_str::SmolStr;
use xh_engine::name::PackageName;
use xh_reports::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum ProjectFormat {
    Dot,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub enum PackageFormat {
    Human,
    Json,
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

#[derive(Debug, IntoReport)]
#[message("unexpected value for {flag}")]
#[suggestion("provide {expected} as the value")]
#[context(found)]
pub struct UnexpectedValueError {
    expected: &'static str,
    #[message]
    flag: &'static str,
    found: SmolStr,
}

#[derive(Debug, IntoReport)]
#[message("unexpected subcommand")]
#[suggestion("provide {expected} as a subcommand")]
#[context(found)]
pub struct UnexpectedSubcommandError {
    expected: &'static str,
    found: Option<String>,
}

#[derive(Debug, Default, IntoReport)]
#[message("could not parse arguments")]
pub struct ParseError;

fn parse_pkgs(args: &mut Arguments) -> Result<Vec<PackageName>, ()> {
    std::iter::repeat(())
        .map(|()| args.opt_free_from_str())
        .take_while(|x| !matches!(x, Ok(None)))
        .map(|x| x.map(|opt| opt.unwrap()).erased())
        .collect::<Result<Vec<_>, _>>()
        .erased()
}

fn parse_path_or_cwd(args: &mut Arguments, flag: &FlagDefinition) -> Result<PathBuf, ()> {
    Ok(
        match args
            .opt_value_from_str(<[_; _]>::from(flag.flags))
            .erased()?
        {
            Some(x) => x,
            None => std::env::current_dir().erased()?,
        },
    )
}

#[derive(Clone, Copy)]
pub enum ArgumentDefinition {
    Root(RootDefinition),
    Flag(FlagDefinition),
    Subcommand(SubcommandDefinition),
}

#[derive(Clone, Copy)]
pub struct Flags {
    pub long: &'static str,
    pub short: Option<&'static str>,
}

impl From<Flags> for [&'static str; 2] {
    fn from(value: Flags) -> Self {
        let Some(short) = value.short else {
            panic!("flag should only be converted to [_; 2] when a short flag is set");
        };

        [short, value.long]
    }
}

impl From<Flags> for [&'static str; 1] {
    fn from(value: Flags) -> Self {
        if let None = value.short {
            panic!("flag should only be converted to [_; 1] when no short flag is set");
        }

        [value.long]
    }
}

#[derive(Clone, Copy)]
pub struct FlagDefinition {
    pub flags: Flags,
    pub description: &'static str,
    pub spec: &'static str,
}

#[derive(Clone, Copy)]
pub struct SubcommandDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub free_spec: Option<&'static str>,
    pub children: &'static [ArgumentDefinition],
}

#[derive(Clone, Copy)]
pub struct RootDefinition {
    pub program: &'static str,
    pub version: &'static str,
    pub children: &'static [ArgumentDefinition],
}

impl InspectAction {
    const FORMAT: FlagDefinition = FlagDefinition {
        flags: Flags {
            long: "--format",
            short: Some("-f"),
        },
        description: "Data output format",
        spec: "<FORMAT>",
    };

    const PROJECT: SubcommandDefinition = SubcommandDefinition {
        name: "project",
        description: "Inspects the given project",
        children: &[ArgumentDefinition::Flag(Self::FORMAT)],
        free_spec: None,
    };

    const PACKAGES: SubcommandDefinition = SubcommandDefinition {
        name: "package",
        description: "Inspects the given package",
        children: &[ArgumentDefinition::Flag(Self::FORMAT)],
        free_spec: Some("[PACKAGE]..."),
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        match args.subcommand().erased()? {
            Some(v) if v == Self::PROJECT.name => {
                path.push(Self::PROJECT);
                let format = args
                    .opt_value_from_fn(<[_; _]>::from(Self::FORMAT.flags), |value| match value {
                        "graphviz" => Ok(ProjectFormat::Dot),
                        "json" => Ok(ProjectFormat::Json),
                        _ => Err(UnexpectedValueError {
                            expected: r#""graphviz" or "json""#,
                            flag: Self::FORMAT.flags.long,
                            found: value.into(),
                        }
                        .into_report()
                        .erased()),
                    })
                    .erased()?
                    .unwrap_or(ProjectFormat::Dot);

                Ok(Self::Project { format })
            }
            Some(v) if v == Self::PACKAGES.name => {
                path.push(Self::PACKAGES);
                let format = args
                    .opt_value_from_fn(<[_; _]>::from(Self::FORMAT.flags), |value| match value {
                        "human" => Ok(PackageFormat::Human),
                        "json" => Ok(PackageFormat::Json),
                        _ => Err(UnexpectedValueError {
                            expected: r#""human" or "json""#,
                            flag: Self::FORMAT.flags.long,
                            found: value.into(),
                        }
                        .into_report()
                        .erased()),
                    })
                    .erased()?
                    .unwrap_or(PackageFormat::Human);

                Ok(Self::Packages {
                    packages: parse_pkgs(args)?,
                    format,
                })
            }
            found => Err(UnexpectedSubcommandError {
                expected: r#""project" or "packages""#,
                found,
            }
            .into_report()
            .erased()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LinkAction {
    Add,
    Delete,
}

impl LinkAction {
    const ADD: SubcommandDefinition = SubcommandDefinition {
        name: "add",
        description: "Add a package to the system's active links",
        children: &[],
        free_spec: None,
    };

    const DELETE: SubcommandDefinition = SubcommandDefinition {
        name: "delete",
        description: "Delete a package from the system's active links",
        children: &[],
        free_spec: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        match args.subcommand().erased()? {
            Some(v) if v == Self::ADD.name => {
                path.push(Self::ADD);
                Ok(Self::Add)
            }
            Some(v) if v == Self::DELETE.name => {
                path.push(Self::DELETE);
                Ok(Self::Delete)
            }
            found => Err(UnexpectedSubcommandError {
                expected: r#""add" or "delete""#,
                found,
            }
            .into_report()
            .erased()),
        }
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
    const DRY_RUN: FlagDefinition = FlagDefinition {
        flags: Flags {
            long: "--dry-run",
            short: Some("-d"),
        },
        description: "Run without making modifications to the system",
        spec: "<true|false>",
    };

    const ROOT: FlagDefinition = FlagDefinition {
        flags: Flags {
            long: "--root",
            short: None,
        },
        description: "Root filesystem to operate on",
        spec: "<PATH>",
    };

    const LINK: SubcommandDefinition = SubcommandDefinition {
        name: "link",
        description: "Manage system links",
        children: &[
            ArgumentDefinition::Flag(Self::DRY_RUN),
            ArgumentDefinition::Flag(Self::ROOT),
            ArgumentDefinition::Subcommand(LinkAction::ADD),
            ArgumentDefinition::Subcommand(LinkAction::DELETE),
        ],
        free_spec: Some("[PACKAGE]..."),
    };

    const BUILD: SubcommandDefinition = SubcommandDefinition {
        name: "build",
        description: "Build packages",
        children: &[ArgumentDefinition::Flag(Self::DRY_RUN)],
        free_spec: Some("[PACKAGE]..."),
    };

    const INSPECT: SubcommandDefinition = SubcommandDefinition {
        name: "inspect",
        description: "Inspect a package or project",
        children: &[],
        free_spec: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        let dry_run = |args: &mut Arguments| {
            args.opt_value_from_str(<[_; _]>::from(Self::DRY_RUN.flags))
                .erased()
                .map(|value| value.unwrap_or(false))
        };

        match args.subcommand().erased()? {
            Some(v) if v == Self::LINK.name => {
                path.push(Self::LINK);
                Ok(Self::Link {
                    dry_run: dry_run(args)?,
                    root: args
                        .value_from_str(<[_; _]>::from(Self::ROOT.flags))
                        .erased()?,
                    action: LinkAction::parse(args, path)?,
                    packages: parse_pkgs(args)?,
                })
            }
            Some(v) if v == Self::BUILD.name => {
                path.push(Self::BUILD);
                Ok(Self::Build {
                    dry_run: dry_run(args)?,
                    packages: parse_pkgs(args)?,
                })
            }
            Some(v) if v == Self::INSPECT.name => {
                path.push(Self::INSPECT);
                Ok(Self::Inspect(InspectAction::parse(args, path)?))
            }
            found => Err(UnexpectedSubcommandError {
                expected: r#""link", "build", or "inspect""#,
                found,
            }
            .into_report()
            .erased()),
        }
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
    const PATH: FlagDefinition = FlagDefinition {
        flags: Flags {
            long: "--path",
            short: Some("-p"),
        },
        description: "Path to the archive or directory",
        spec: "[PATH]",
    };

    const PACK: SubcommandDefinition = SubcommandDefinition {
        name: "pack",
        description: "Pack a directory into an archive",
        children: &[ArgumentDefinition::Flag(Self::PATH)],
        free_spec: None,
    };

    const UNPACK: SubcommandDefinition = SubcommandDefinition {
        name: "unpack",
        description: "Unpack an archive into a directory",
        children: &[ArgumentDefinition::Flag(Self::PATH)],
        free_spec: None,
    };

    const DECODE: SubcommandDefinition = SubcommandDefinition {
        name: "decode",
        description: "Decode an archive into events",
        children: &[],
        free_spec: None,
    };

    const HASH: SubcommandDefinition = SubcommandDefinition {
        name: "hash",
        description: "Hash an archive",
        children: &[],
        free_spec: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        match args.subcommand().erased()? {
            Some(v) if v == Self::PACK.name => {
                path.push(Self::PACK);
                Ok(Self::Pack {
                    path: parse_path_or_cwd(args, &Self::PATH)?,
                })
            }
            Some(v) if v == Self::UNPACK.name => {
                path.push(Self::UNPACK);
                Ok(Self::Unpack {
                    path: parse_path_or_cwd(args, &Self::PATH)?,
                })
            }
            Some(v) if v == Self::DECODE.name => {
                path.push(Self::DECODE);
                Ok(Self::Decode)
            }
            Some(v) if v == Self::HASH.name => {
                path.push(Self::HASH);
                Ok(Self::Hash)
            }
            found => Err(UnexpectedSubcommandError {
                expected: r#""pack", "unpack", "decode", or "hash""#,
                found,
            }
            .into_report()
            .erased()),
        }
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
    const PROJECT: FlagDefinition = FlagDefinition {
        flags: Flags {
            long: "--project",
            short: Some("-p"),
        },
        description: "Path to the target project",
        spec: "<PATH>",
    };

    const PACKAGE: SubcommandDefinition = SubcommandDefinition {
        name: "package",
        description: "Manage packages",
        children: &[
            ArgumentDefinition::Subcommand(PackageAction::INSPECT),
            ArgumentDefinition::Subcommand(PackageAction::BUILD),
            ArgumentDefinition::Subcommand(PackageAction::LINK),
            ArgumentDefinition::Flag(Self::PROJECT),
        ],
        free_spec: None,
    };

    const ARCHIVE: SubcommandDefinition = SubcommandDefinition {
        name: "archive",
        description: "Manage archives",
        children: &[],
        free_spec: None,
    };

    pub const ROOT: RootDefinition = RootDefinition {
        program: env!("CARGO_BIN_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        children: &[
            ArgumentDefinition::Subcommand(Self::PACKAGE),
            ArgumentDefinition::Subcommand(Self::ARCHIVE),
        ],
    };

    pub fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        match args.subcommand().erased()? {
            Some(v) if v == Self::PACKAGE.name => {
                path.push(Self::PACKAGE);
                Ok(Self::Package {
                    project: parse_path_or_cwd(args, &Self::PROJECT)?,
                    action: PackageAction::parse(args, path)?,
                })
            }
            Some(v) if v == Self::ARCHIVE.name => {
                path.push(Self::ARCHIVE);
                Ok(Self::Archive(ArchiveAction::parse(args, path)?))
            }
            found => Err(UnexpectedSubcommandError {
                expected: r#""package" or "archive""#,
                found,
            }
            .into_report()
            .erased()),
        }
    }
}

fn with_children(
    argument: &ArgumentDefinition,
) -> impl DoubleEndedIterator<Item = &ArgumentDefinition> {
    let children = match argument {
        ArgumentDefinition::Root(root) => root.children,
        ArgumentDefinition::Flag(_) => unreachable!(),
        ArgumentDefinition::Subcommand(subcommand) => subcommand.children,
    };
    let children = children
        .iter()
        .filter(|child| matches!(child, ArgumentDefinition::Flag(_)));

    std::iter::once(argument).chain(children)
}

fn with_positional<T, I, J>(iter: I) -> impl ExactSizeIterator<Item = (bool, bool, T)>
where
    I: IntoIterator<IntoIter = J>,
    J: ExactSizeIterator<Item = T>,
{
    let iter = iter.into_iter();
    let len = iter.len();
    iter.enumerate()
        .map(move |(i, frag)| (i == 0, i == len - 1, frag))
}

#[derive(Debug)]
pub enum SpecFragment {
    Program(&'static str),
    Flag(&'static str),
    Subcommand(&'static str),
    Other(&'static str),
}

pub fn assemble_spec<'a>(path: &[ArgumentDefinition]) -> VecDeque<SpecFragment> {
    let mut fragments = VecDeque::with_capacity(path.len());

    for argument in path.iter().flat_map(with_children).rev() {
        match argument {
            ArgumentDefinition::Root(root) => {
                fragments.push_front(SpecFragment::Program(root.program));
            }
            ArgumentDefinition::Flag(flag) => {
                fragments.push_front(SpecFragment::Other(flag.spec));
                fragments.push_front(SpecFragment::Flag(flag.flags.long));
            }
            ArgumentDefinition::Subcommand(subcommand) => {
                fragments.push_front(SpecFragment::Subcommand(subcommand.name));
                if let Some(free_spec) = subcommand.free_spec {
                    fragments.push_back(SpecFragment::Other(free_spec));
                }
            }
        }
    }

    fragments
}

#[derive(Debug)]
pub enum HelpFragment {
    CategorySegment {
        first: bool,
        last: bool,
        segment: &'static str,
    },
    ArgumentSegment {
        first: bool,
        last: bool,
        segment: &'static str,
    },
    Description {
        last: bool,
        text: &'static str,
    },
    About {
        program: &'static str,
        version: &'static str,
    },
}

pub fn assemble_help(path: &[ArgumentDefinition]) -> Vec<HelpFragment> {
    let mut fragments = Vec::with_capacity(path.len());
    let mut about = None;

    for i in (0..path.len()).rev() {
        let arguments = &path[..=i];
        let current_argument = &path[i];
        let last_argument = arguments.last().unwrap();

        if let ArgumentDefinition::Root(root) = current_argument {
            about = Some(HelpFragment::About {
                program: root.program,
                version: root.version,
            });
        }

        for (first, last, argument) in with_positional(arguments) {
            let segment = match argument {
                ArgumentDefinition::Root(root) => root.program,
                ArgumentDefinition::Subcommand(subcommand) => subcommand.name,
                ArgumentDefinition::Flag(_) => panic!("flag argument should not be within path"),
            };

            fragments.push(HelpFragment::CategorySegment {
                first,
                last,
                segment,
            });
        }

        let children = match last_argument {
            ArgumentDefinition::Root(root) => root.children,
            ArgumentDefinition::Subcommand(subcommand) => subcommand.children,
            ArgumentDefinition::Flag(_) => panic!("flag argument should not be within path"),
        };

        for (_, last, child) in with_positional(children) {
            let description = match child {
                ArgumentDefinition::Flag(flag) => {
                    let has_short = if let Some(short) = flag.flags.short {
                        fragments.push(HelpFragment::ArgumentSegment {
                            last: false,
                            first: true,
                            segment: short,
                        });

                        true
                    } else {
                        false
                    };

                    fragments.push(HelpFragment::ArgumentSegment {
                        last: true,
                        first: !has_short,
                        segment: flag.flags.long,
                    });

                    flag.description
                }
                ArgumentDefinition::Subcommand(subcommand) => {
                    fragments.push(HelpFragment::ArgumentSegment {
                        last: true,
                        first: true,
                        segment: subcommand.name,
                    });

                    subcommand.description
                }
                ArgumentDefinition::Root(_) => panic!("child argument should not be root"),
            };

            fragments.push(HelpFragment::Description {
                text: description,
                last,
            });
        }
    }

    if let Some(about) = about {
        fragments.push(about);
    }

    fragments
}

pub fn format_spec<'a>(
    mut writer: impl fmt::Write,
    spec: impl Iterator<Item = &'a SpecFragment>,
) -> fmt::Result {
    for fragment in spec {
        let value = match fragment {
            SpecFragment::Program(v)
            | SpecFragment::Flag(v)
            | SpecFragment::Subcommand(v)
            | SpecFragment::Other(v) => v,
        };

        write!(writer, "{value} ")?;
    }

    writeln!(writer)
}

pub fn format_help<'a>(
    mut writer: impl fmt::Write,
    help: impl Iterator<Item = &'a HelpFragment> + Clone,
) -> fmt::Result {
    let max = help
        .clone()
        .map(|fragment| {
            if let HelpFragment::ArgumentSegment { segment, .. } = fragment {
                segment.len()
            } else {
                0
            }
        })
        .max()
        .unwrap_or(0);
    let argument_padding = max + 1;

    for fragment in help {
        match fragment {
            HelpFragment::CategorySegment { last, segment, .. } => {
                let rpad = if *last { ":\n" } else { " " };
                write!(writer, "{segment}{rpad}")?;
            }
            HelpFragment::ArgumentSegment {
                first,
                last,
                segment,
            } => {
                let lpad = if *first { "  " } else { "" };
                if *last {
                    write!(writer, "{lpad}{segment:argument_padding$}")
                } else {
                    write!(writer, "{lpad}{segment}, ")
                }?;
            }
            HelpFragment::Description { last, text } => {
                let rpad = if *last { "\n" } else { "" };
                writeln!(writer, " {text}{rpad}")?;
            }
            HelpFragment::About { program, version } => {
                write!(writer, "{program} v{version}")?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use owo_colors::OwoColorize;

    use super::*;

    #[test]
    fn test_spec() {
        let args = ["package"];

        let mut args = Arguments::from_vec(args.into_iter().map(OsString::from).collect());
        let help = args.contains("--help");

        let mut path = vec![];
        let action = Action::parse(&mut args, &mut path);

        if action.is_err() || help {
            let path: Vec<_> = std::iter::once(ArgumentDefinition::Root(Action::ROOT))
                .chain(path.into_iter().map(ArgumentDefinition::Subcommand))
                .collect();

            let spec = assemble_spec(&path);
            let mut spec_msg = String::new();
            format_spec(&mut spec_msg, spec.iter()).unwrap();

            let help = assemble_help(&path);
            let mut help_msg = String::new();
            format_help(&mut help_msg, help.iter()).unwrap();

            eprintln!("{}\n{help_msg}", spec_msg.bold());
        }

        panic!();
    }
}

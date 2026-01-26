use std::{collections::VecDeque, env, fmt, path::PathBuf};

use pico_args::Arguments;
use smol_str::SmolStr;
use tracing::error;
use xh_engine::name::PackageName;
use xh_reports::prelude::*;

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
    expected: SmolStr,
    found: Option<String>,
}

macro_rules! flag_call {
    ($method:ident($args:expr, $flags:expr $(, $extra:expr)*)) => {{
        match $flags.short {
            Some(short) => $args.$method([short, $flags.long] $(, $extra)*),
            None => $args.$method($flags.long $(, $extra)*),
        }
    }};
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

#[derive(Clone, Copy)]
pub struct FlagDefinition {
    pub flags: Flags,
    pub description: &'static str,
    pub spec: &'static str,
}

#[derive(Clone, Copy)]
pub struct SubcommandDefinition {
    pub name: &'static str,
    pub summary: &'static str,
    pub description: Option<&'static str>,
    pub free_spec: Option<&'static str>,
    pub children: &'static [ArgumentDefinition],
}

// TODO: include source hash
#[derive(Clone, Copy)]
pub struct RootDefinition {
    pub program: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub children: &'static [ArgumentDefinition],
}

#[derive(Debug)]
pub enum SpecFragment {
    Program(&'static str),
    Flag(&'static str),
    Subcommand(&'static str),
    Description(&'static str),
    Syntax(&'static str),
}

#[derive(Debug)]
pub enum HelpFragment {
    CategoryFinish,
    CategoryLabelSegment {
        first: bool,
        last: bool,
        segment: &'static str,
    },
    CategoryValue {
        first: bool,
        last: bool,
        value: &'static str,
    },
    Description {
        text: &'static str,
    },
    About {
        program: &'static str,
        version: &'static str,
    },
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

pub fn assemble_spec<'a>(path: &[ArgumentDefinition]) -> VecDeque<SpecFragment> {
    let mut fragments = VecDeque::with_capacity(path.len());
    for argument in path.iter().flat_map(with_children).rev() {
        match argument {
            ArgumentDefinition::Root(root) => {
                fragments.push_front(SpecFragment::Program(root.program));
            }
            ArgumentDefinition::Flag(flag) => {
                fragments.push_front(SpecFragment::Syntax(flag.spec));
                fragments.push_front(SpecFragment::Flag(flag.flags.long));
            }
            ArgumentDefinition::Subcommand(subcommand) => {
                fragments.push_front(SpecFragment::Subcommand(subcommand.name));
                if let Some(free_spec) = subcommand.free_spec {
                    fragments.push_back(SpecFragment::Syntax(free_spec));
                }
            }
        }
    }

    if let Some(last) = path.last() {
        fragments.push_back(SpecFragment::Description(match last {
            ArgumentDefinition::Root(root) => root.description,
            ArgumentDefinition::Subcommand(subcommand) => {
                subcommand.description.unwrap_or(subcommand.summary)
            }
            ArgumentDefinition::Flag(_) => panic!("flag argument should not be within path"),
        }));
    }

    fragments
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

            fragments.push(HelpFragment::CategoryLabelSegment {
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

        for child in children {
            let text = match child {
                ArgumentDefinition::Flag(flag) => {
                    let has_short = if let Some(short) = flag.flags.short {
                        fragments.push(HelpFragment::CategoryValue {
                            last: false,
                            first: true,
                            value: short,
                        });

                        true
                    } else {
                        false
                    };

                    fragments.push(HelpFragment::CategoryValue {
                        last: true,
                        first: !has_short,
                        value: flag.flags.long,
                    });

                    flag.description
                }
                ArgumentDefinition::Subcommand(subcommand) => {
                    fragments.push(HelpFragment::CategoryValue {
                        last: true,
                        first: true,
                        value: subcommand.name,
                    });

                    subcommand.summary
                }
                ArgumentDefinition::Root(_) => panic!("child argument should not be root"),
            };

            fragments.push(HelpFragment::Description { text });
        }

        fragments.push(HelpFragment::CategoryFinish);
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
        match fragment {
            SpecFragment::Program(v)
            | SpecFragment::Flag(v)
            | SpecFragment::Subcommand(v)
            | SpecFragment::Syntax(v) => write!(writer, "{v} "),
            SpecFragment::Description(description) => writeln!(writer, "\n{description}"),
        }?;
    }

    Ok(())
}

pub fn format_help(mut writer: impl fmt::Write, help: &[HelpFragment]) -> fmt::Result {
    const CATEGORY_VALUE_SEPARATOR: &'static str = ", ";

    // max padding needed for description alignment
    let padding = help
        .iter()
        .scan(0, |current, fragment| {
            let mut length = 0;
            if let HelpFragment::CategoryValue { value, last, .. } = fragment {
                *current += value.len();
                if *last {
                    length = *current;
                    *current = 0;
                } else {
                    *current += CATEGORY_VALUE_SEPARATOR.len();
                }
            }

            Some(length)
        })
        .max()
        .unwrap_or(0);

    let mut i = 0;
    let mut category_length = 0;
    while let Some(fragment) = help.get(i) {
        match fragment {
            HelpFragment::CategoryFinish => writeln!(writer)?,
            HelpFragment::CategoryLabelSegment {
                first: _,
                last,
                segment,
            } => {
                let rpad = if *last { ":\n" } else { " " };
                write!(writer, "{segment}{rpad}")?;
            }
            HelpFragment::CategoryValue { value, first, last } => {
                category_length += value.len();

                let lpad = if *first { "  " } else { "" };
                write!(writer, "{lpad}{value}")?;

                if *last {
                    let final_padding = padding - category_length;
                    category_length = 0;

                    write!(writer, "{:final_padding$}", "")?;
                } else {
                    category_length += CATEGORY_VALUE_SEPARATOR.len();
                    writer.write_str(CATEGORY_VALUE_SEPARATOR)?;
                }
            }
            HelpFragment::Description { text } => {
                writeln!(writer, " {text}")?;
            }
            HelpFragment::About { program, version } => {
                write!(writer, "{program} v{version}")?;
            }
        }

        i += 1;
    }

    Ok(())
}

macro_rules! parse_subcommand {
    ($args:expr, $path:expr, [$(($flag:expr, $func:expr)),*]) => {{
        match $args.subcommand().erased()? {
            $(Some(v) if v == $flag.name => {
                $path.push($flag);
                $func($args, $path)
            })*
            found => Err(UnexpectedSubcommandError {
                expected: {
                    const COUNT: usize = [$($flag),*].len();

                    let mut builder = smol_str::SmolStrBuilder::new();
                    let mut i = 0;
                    $(#[allow(unused_assignments)] {
                        builder.push('"');
                        builder.push_str($flag.name);
                        builder.push('"');

                        let last = i == COUNT - 1;
                        if !last {
                            builder.push_str(" or ")
                        }

                        i += 1;
                    })*

                    builder.finish()
                },
                found,
            }
            .into_report()
            .erased()),
        }
    }};
}

fn parse_pkgs(args: &mut Arguments) -> Result<Vec<PackageName>, ()> {
    std::iter::repeat_with(|| args.opt_free_from_str())
        .take_while(|x| !matches!(x, Ok(None)))
        .map(|x| x.map(Option::unwrap))
        .collect::<StdResult<Vec<_>, _>>()
        .erased()
}

fn parse_path_or_cwd(args: &mut Arguments, definition: &FlagDefinition) -> Result<PathBuf, ()> {
    Ok(
        match flag_call!(opt_value_from_str(args, definition.flags)).erased()? {
            Some(x) => x,
            None => std::env::current_dir().erased()?,
        },
    )
}

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
        summary: "Inspects the given project",
        children: &[ArgumentDefinition::Flag(Self::FORMAT)],
        free_spec: None,
        description: None,
    };

    const PACKAGES: SubcommandDefinition = SubcommandDefinition {
        name: "package",
        summary: "Inspects the given package",
        children: &[ArgumentDefinition::Flag(Self::FORMAT)],
        free_spec: Some("[PACKAGE]..."),
        description: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        let project = |args: &mut Arguments, _| {
            let format = flag_call!(opt_value_from_fn(
                args,
                Self::FORMAT.flags,
                |value| match value {
                    "graphviz" => Ok(ProjectFormat::Dot),
                    "json" => Ok(ProjectFormat::Json),
                    _ => Err(UnexpectedValueError {
                        expected: r#""graphviz" or "json""#,
                        flag: Self::FORMAT.flags.long,
                        found: value.into(),
                    }
                    .into_report()
                    .erased()),
                }
            ))
            .erased()?
            .unwrap_or(ProjectFormat::Dot);

            Ok(Self::Project { format })
        };

        let packages = |args: &mut Arguments, _| {
            let format = flag_call!(opt_value_from_fn(
                args,
                Self::FORMAT.flags,
                |value| match value {
                    "human" => Ok(PackageFormat::Human),
                    "json" => Ok(PackageFormat::Json),
                    _ => Err(UnexpectedValueError {
                        expected: r#""human" or "json""#,
                        flag: Self::FORMAT.flags.long,
                        found: value.into(),
                    }
                    .into_report()
                    .erased()),
                }
            ))
            .erased()?
            .unwrap_or(PackageFormat::Human);

            Ok(Self::Packages {
                packages: parse_pkgs(args)?,
                format,
            })
        };

        parse_subcommand!(
            args,
            path,
            [
                (InspectAction::PROJECT, project),
                (InspectAction::PACKAGES, packages)
            ]
        )
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
        summary: "Add a package to the system's active links",
        children: &[],
        free_spec: None,
        description: None,
    };

    const DELETE: SubcommandDefinition = SubcommandDefinition {
        name: "delete",
        summary: "Delete a package from the system's active links",
        children: &[],
        free_spec: None,
        description: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        parse_subcommand!(
            args,
            path,
            [
                (LinkAction::ADD, |_, _| Ok(Self::Add)),
                (LinkAction::DELETE, |_, _| Ok(Self::Delete))
            ]
        )
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
        summary: "Manage system links",
        children: &[
            ArgumentDefinition::Flag(Self::DRY_RUN),
            ArgumentDefinition::Flag(Self::ROOT),
            ArgumentDefinition::Subcommand(LinkAction::ADD),
            ArgumentDefinition::Subcommand(LinkAction::DELETE),
        ],
        free_spec: Some("[PACKAGE]..."),
        description: None,
    };

    const BUILD: SubcommandDefinition = SubcommandDefinition {
        name: "build",
        summary: "Build packages",
        children: &[ArgumentDefinition::Flag(Self::DRY_RUN)],
        free_spec: Some("[PACKAGE]..."),
        description: None,
    };

    const INSPECT: SubcommandDefinition = SubcommandDefinition {
        name: "inspect",
        summary: "Inspect a package or project",
        children: &[],
        free_spec: None,
        description: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        let dry_run = |args: &mut Arguments| {
            flag_call!(opt_value_from_str(args, Self::DRY_RUN.flags))
                .erased()
                .map(|value| value.unwrap_or(false))
        };

        parse_subcommand!(
            args,
            path,
            [
                (PackageAction::LINK, |args: &mut Arguments, path| Ok(
                    Self::Link {
                        dry_run: dry_run(args)?,
                        root: flag_call!(value_from_str(args, Self::ROOT.flags)).erased()?,
                        action: LinkAction::parse(args, path)?,
                        packages: parse_pkgs(args)?,
                    }
                )),
                (PackageAction::BUILD, |args: &mut Arguments, _| Ok(
                    Self::Build {
                        dry_run: dry_run(args)?,
                        packages: parse_pkgs(args)?,
                    }
                )),
                (PackageAction::INSPECT, |args, path| Ok(Self::Inspect(
                    InspectAction::parse(args, path)?
                )))
            ]
        )
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
        summary: "Pack a directory into an archive",
        children: &[ArgumentDefinition::Flag(Self::PATH)],
        free_spec: None,
        description: None,
    };

    const UNPACK: SubcommandDefinition = SubcommandDefinition {
        name: "unpack",
        summary: "Unpack an archive into a directory",
        children: &[ArgumentDefinition::Flag(Self::PATH)],
        free_spec: None,
        description: None,
    };

    const DECODE: SubcommandDefinition = SubcommandDefinition {
        name: "decode",
        summary: "Decode an archive into events",
        children: &[],
        free_spec: None,
        description: None,
    };

    const HASH: SubcommandDefinition = SubcommandDefinition {
        name: "hash",
        summary: "Hash an archive",
        children: &[],
        free_spec: None,
        description: None,
    };

    fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        parse_subcommand!(
            args,
            path,
            [
                (ArchiveAction::PACK, |args, _| Ok(Self::Pack {
                    path: parse_path_or_cwd(args, &Self::PATH)?,
                })),
                (ArchiveAction::UNPACK, |args, _| Ok(Self::Unpack {
                    path: parse_path_or_cwd(args, &Self::PATH)?,
                })),
                (ArchiveAction::DECODE, |_, _| Ok(Self::Decode)),
                (ArchiveAction::HASH, |_, _| Ok(Self::Hash))
            ]
        )
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
        summary: "Manage packages",
        children: &[
            ArgumentDefinition::Subcommand(PackageAction::INSPECT),
            ArgumentDefinition::Subcommand(PackageAction::BUILD),
            ArgumentDefinition::Subcommand(PackageAction::LINK),
            ArgumentDefinition::Flag(Self::PROJECT),
        ],
        free_spec: None,
        description: None,
    };

    const ARCHIVE: SubcommandDefinition = SubcommandDefinition {
        name: "archive",
        summary: "Manage archives",
        children: &[
            ArgumentDefinition::Subcommand(ArchiveAction::DECODE),
            ArgumentDefinition::Subcommand(ArchiveAction::HASH),
            ArgumentDefinition::Subcommand(ArchiveAction::PACK),
            ArgumentDefinition::Subcommand(ArchiveAction::UNPACK),
        ],
        free_spec: None,
        description: None,
    };

    pub const ROOT: RootDefinition = RootDefinition {
        program: env!("CARGO_BIN_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        children: &[
            ArgumentDefinition::Subcommand(Self::PACKAGE),
            ArgumentDefinition::Subcommand(Self::ARCHIVE),
        ],
        description: "A build system framework and package manager inspired by NixOS.",
    };

    pub fn parse(args: &mut Arguments, path: &mut Vec<SubcommandDefinition>) -> Result<Self, ()> {
        parse_subcommand!(
            args,
            path,
            [
                (Action::PACKAGE, |args: &mut Arguments, path| Ok(
                    Self::Package {
                        project: parse_path_or_cwd(args, &Self::PROJECT).erased()?,
                        action: PackageAction::parse(args, path).erased()?,
                    }
                )),
                (Action::ARCHIVE, |args, path| Ok(Self::Archive(
                    ArchiveAction::parse(args, path).erased()?
                )))
            ]
        )
    }

    /// Parses the command line arguments, printing help and exiting if either:
    /// - There was an error during parsing
    /// - The user included `--help` in the arguments
    /// Note that this function may call [`std::process::exit`], ignoring all destructors.
    pub fn parse_or_exit() -> Action {
        let mut args = Arguments::from_env();
        let mut path = vec![];
        let help = args.contains("--help");
        let action = Action::parse(&mut args, &mut path).and_then(|action| {
            let rest = args.finish();
            rest.is_empty().then_some(action).ok_or_else(|| {
                let arguments = rest
                    .into_iter()
                    .map(|argument| Frame::context("argument", argument.display()));
                Report::new("encountered unrecognized arguments")
                    .with_frames(arguments)
                    .with_frame(Frame::suggestion("use --help to view valid invocations"))
            })
        });

        let exit_code = match action {
            Ok(action) if !help => return action,
            Ok(_) => 0,
            Err(report) => {
                error!(
                    report = &report.into_error() as &dyn StdError,
                    suggestion = "use --help to view valid invocations",
                    "could not parse command-line arguments",
                );

                1
            }
        };

        let path: Vec<_> = std::iter::once(ArgumentDefinition::Root(Action::ROOT))
            .chain(path.into_iter().map(ArgumentDefinition::Subcommand))
            .collect();

        let spec = assemble_spec(&path);
        let mut spec_msg = String::new();
        format_spec(&mut spec_msg, spec.iter()).unwrap();

        let help = assemble_help(&path);
        let mut help_msg = String::new();
        format_help(&mut help_msg, &help).unwrap();

        eprintln!("{spec_msg}\n{help_msg}");
        std::process::exit(exit_code)
    }
}

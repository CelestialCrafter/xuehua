use bytes::{Buf, Bytes, TryGetError};
use educe::Educe;
use smol_str::{SmolStr, SmolStrBuilder};
use xh_reports::prelude::*;

#[derive(Debug, IntoReport)]
#[message("unrecognized prefix")]
#[context(0 = prefix)]
pub struct UnrecognizedPrefix(char);

#[derive(Debug, IntoReport)]
#[message("unrecognized package type")]
#[context(0 = ty)]
pub struct UnrecognizedPackageType(SmolStr);

#[derive(Default, Debug, IntoReport)]
#[message("missing field {0}")]
pub struct MissingField(#[format(message)] &'static str);

#[derive(Default, Debug, IntoReport)]
#[message("could not decode index")]
#[suggestion(
    "provide an index that conforms to https://wiki.alpinelinux.org/wiki/Apk_spec#APKINDEX_Format"
)]
pub struct Error;

#[derive(Debug, Clone)]
pub enum Operator {
    GreaterEqual,
    LessEqual,
    Range,
    Equal,
    Greater,
    Less,
}

#[derive(Debug, Clone)]
pub struct Version {
    pub version: SmolStr,
    pub operator: Operator,
}

#[derive(Debug, Clone, Educe)]
#[educe(PartialEq, Eq)]
pub struct PackageReference {
    pub name: SmolStr,
    #[educe(Eq(ignore))]
    pub version: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    PackageName(SmolStr),
    Version(SmolStr),
    Dependencies(Vec<PackageReference>),
    Provides(Vec<PackageReference>),
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: SmolStr,
    pub version: SmolStr,
    pub dependencies: Vec<PackageReference>,
    pub provides: Vec<PackageReference>,
}

pub struct Decoder;

impl Decoder {
    pub fn decode(buffer: &mut Bytes) -> impl Iterator<Item = Result<Package, Error>> {
        std::iter::from_fn(|| {
            if buffer.is_empty() {
                None
            } else {
                let mut attempt = buffer.clone();
                Some(process_package(&mut attempt).inspect(|_| *buffer = attempt))
            }
        })
    }
}

fn process_package(buffer: &mut Bytes) -> Result<Package, Error> {
    let mut name = None;
    let mut version = None;
    let mut dependencies = Vec::new();
    let mut provides = Vec::new();

    while let Some(field) = process_field(buffer)? {
        eprintln!("decoded field: {field:?}");
        match field {
            Field::PackageName(v) => name = Some(v),
            Field::Version(v) => version = Some(v),
            Field::Dependencies(v) => dependencies.extend(v),
            Field::Provides(v) => provides.extend(v),
            Field::Ignored => (),
        }

        if buffer.starts_with(b"\n") {
            buffer.advance(1);
            break;
        }
    }

    match (name, version) {
        (None, _) => Err(MissingField("package name")),
        (_, None) => Err(MissingField("package version")),
        (Some(name), Some(version)) => Ok(Package {
            name,
            version,
            dependencies,
            provides,
        }),
    }
    .wrap()
}

fn process_field(buffer: &mut Bytes) -> Result<Option<Field>, Error> {
    let mut prefix = [0; 2];
    buffer.try_copy_to_slice(&mut prefix).wrap()?;
    let line = process_line(buffer)?;

    let field = match prefix {
        [b'C', b':'] => Field::Ignored,
        [b'P', b':'] => Field::PackageName(line),
        [b'V', b':'] => Field::Version(line),
        [b'A', b':'] => Field::Ignored,
        [b'S', b':'] => Field::Ignored,
        [b'I', b':'] => Field::Ignored,
        [b'T', b':'] => Field::Ignored,
        [b'U', b':'] => Field::Ignored,
        [b'L', b':'] => Field::Ignored,
        [b'o', b':'] => Field::Ignored,
        [b'm', b':'] => Field::Ignored,
        [b't', b':'] => Field::Ignored,
        [b'c', b':'] => Field::Ignored,
        [b'k', b':'] => Field::Ignored,
        [b'D', b':'] => Field::Dependencies(process_list(&line, process_package_reference)),
        [b'p', b':'] => Field::Provides(process_list(&line, process_package_reference)),
        [b'i', b':'] => Field::Ignored,
        prefix => return Err(UnrecognizedPrefix(prefix[0] as char).wrap()),
    };

    Ok(Some(field))
}

fn process_package_reference(input: &str) -> PackageReference {
    for (op, str) in [
        (Operator::GreaterEqual, ">="),
        (Operator::LessEqual, "<="),
        (Operator::Range, "><"),
        (Operator::Equal, "="),
        (Operator::GreaterEqual, ">"),
        (Operator::LessEqual, "<"),
    ] {
        if let Some((name, version)) = input.split_once(str) {
            return PackageReference {
                name: name.into(),
                version: Some(Version {
                    version: version.into(),
                    operator: op,
                }),
            };
        }
    }

    PackageReference {
        name: input.into(),
        version: None,
    }
}

fn process_list<T>(input: &str, func: impl Fn(&str) -> T) -> Vec<T> {
    input.split(' ').map(func).collect()
}

fn process_line(buffer: &mut Bytes) -> Result<SmolStr, Error> {
    let mut builder = SmolStrBuilder::new();

    let mut searching = true;
    while buffer.has_remaining() && searching {
        let mut lines = buffer.chunk().split(|b| *b == b'\n');

        // `line` goes either to the newline, or to the end of the chunk
        let line = lines
            .next()
            .expect("&[u8]::split() should always return at least 1 element");

        // if theres something after `line`, that means the iterator has split on a newline
        searching = lines.next().is_none();

        let line = str::from_utf8(line).wrap()?;
        builder.push_str(line);

        // if we did find the newline, we need to consume it aswell
        buffer.advance(line.len() + !searching as usize);
    }

    // if we didn't find our newline, error out asking for 1 more byte
    if searching {
        Err(TryGetError {
            requested: 1,
            available: 0,
        }
        .wrap())
    } else {
        Ok(builder.finish())
    }
}

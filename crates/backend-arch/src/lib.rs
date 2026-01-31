use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    fs::read_dir,
    path::Path,
    str::FromStr,
    sync::LazyLock,
};

use alpm_repo_db::desc::RepoDescFile;
use serde::Deserialize;
use smol_str::{SmolStr, ToSmolStr};
use xh_engine::{
    backend::{Backend, Error},
    encoding::to_value,
    executor::Executor,
    gen_name,
    name::{BackendName, PackageName},
    package::{Dependency, DispatchRequest, LinkTime, Metadata, Package},
    planner::{Planner, Unfrozen},
};
use xh_executor_bubblewrap::BubblewrapExecutor;
use xh_executor_compression::CompressionExecutor;
use xh_executor_http::HttpExecutor;
use xh_reports::{partition_results, prelude::*};

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    pub mirror: String,
    pub architecture: SmolStr,
    #[serde(default)]
    pub repos: Vec<SmolStr>,
    #[serde(default)]
    pub priorities: BTreeMap<SmolStr, usize>,
}

pub struct ArchBackend {
    options: Options,
}

impl ArchBackend {
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    fn index_to_packages(
        &self,
        index: HashMap<SmolStr, IndexEntry>,
    ) -> impl Iterator<Item = Result<Package, ()>> {
        let transform_ref = |name, origin| Package {
            name: package_name(name),
            metadata: Metadata,
            requests: vec![],
            dependencies: vec![Dependency {
                name: package_name(origin),
                time: LinkTime::Runtime,
            }],
        };

        let transform_pkg = move |name, dependencies: Vec<_>, repo, file| {
            let pkg = Package {
                name: package_name(name),
                metadata: Metadata,
                requests: vec![
                    DispatchRequest {
                        executor: HttpExecutor::name().clone(),
                        payload: to_value(xh_executor_http::Request {
                            path: "download.pkg.tar.zst".into(),
                            url: FromStr::from_str(&format!(
                                "{}/{repo}/os/{}/{file}",
                                self.options.mirror, self.options.architecture
                            ))
                            .erased()?,
                            method: FromStr::from_str("GET").expect("GET should be a valid method"),
                        })
                        .erased()?,
                    },
                    DispatchRequest {
                        executor: CompressionExecutor::name().clone(),
                        payload: to_value(xh_executor_compression::Request {
                            algorithm: xh_executor_compression::Algorithm::Zstd,
                            action: xh_executor_compression::Action::Decompress,
                            input: "download.pkg.tar.zst".into(),
                            output: "download.pkg.tar".into(),
                        })
                        .erased()?,
                    },
                    DispatchRequest {
                        executor: BubblewrapExecutor::name().clone(),
                        payload: to_value(xh_executor_bubblewrap::Request {
                            program: "/busybox".into(),
                            working_dir: None,
                            arguments: ["tar", "x", "-f", "download.pkg.tar", "-C", "output"]
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                            environment: Vec::new(),
                        })
                        .erased()?,
                    },
                ],
                dependencies: dependencies
                    .into_iter()
                    .map(|dependency| Dependency {
                        name: package_name(dependency),
                        time: LinkTime::Runtime,
                    })
                    .collect(),
            };

            Ok(pkg)
        };

        index.into_iter().map(move |(key, entry)| match entry.ty {
            IndexEntryType::Package {
                dependencies,
                repo,
                file,
            } => transform_pkg(key, dependencies, repo, file),
            IndexEntryType::Reference { origin } => Ok(transform_ref(key, origin)),
        })
    }

    fn resolve_index(&self, descriptions: Vec<Description>) -> HashMap<SmolStr, IndexEntry> {
        fn attempt_replacement(
            name: SmolStr,
            priority: usize,
            create_index_entry: impl FnOnce() -> IndexEntry,
            index: &mut HashMap<SmolStr, IndexEntry>,
        ) {
            match index.entry(name) {
                Entry::Occupied(mut occupied) => {
                    if priority > occupied.get().priority {
                        occupied.insert(create_index_entry());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(create_index_entry());
                }
            }
        }

        let mut index = HashMap::with_capacity(descriptions.len());
        for description in descriptions {
            let Description {
                name,
                dependencies,
                provides,
                file,
                repo,
            } = description;
            let priority = self
                .options
                .priorities
                .get(&name)
                .copied()
                .unwrap_or_default();

            attempt_replacement(
                name.clone(),
                priority,
                || IndexEntry {
                    priority,
                    ty: IndexEntryType::Package {
                        dependencies,
                        file,
                        repo,
                    },
                },
                &mut index,
            );

            for provided in provides {
                attempt_replacement(
                    provided,
                    priority,
                    || IndexEntry {
                        priority,
                        ty: IndexEntryType::Reference {
                            origin: name.clone(),
                        },
                    },
                    &mut index,
                );
            }
        }

        index
    }
}

impl Backend for ArchBackend {
    type Value = ();

    fn name() -> &'static xh_engine::name::BackendName {
        static NAME: LazyLock<BackendName> = LazyLock::new(|| gen_name!(arch@xuehua));
        &*NAME
    }

    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Error> {
        let entries = scan_project(project).wrap()?;
        let index = self.resolve_index(entries);
        let packages = self.index_to_packages(index);
        let planner = packages.map(|result| planner.register(result?).erased().map(|_| ()));

        partition_results::<_, (), _, Vec<_>>(planner)
            .map_err(|reports| Error.into_report().with_children(reports))
    }
}

#[derive(Debug, Default)]
struct Description {
    name: SmolStr,
    repo: SmolStr,
    dependencies: Vec<SmolStr>,
    provides: Vec<SmolStr>,
    file: SmolStr,
}

fn content_to_description(content: &str, repo: SmolStr) -> Result<Description, ()> {
    let (name, dependencies, provides, file_name) =
        match RepoDescFile::from_str(content).erased()? {
            RepoDescFile::V1(v1) => (v1.name, v1.dependencies, v1.provides, v1.file_name),
            RepoDescFile::V2(v2) => (v2.name, v2.dependencies, v2.provides, v2.file_name),
        };

    let transform = |value| match value {
        alpm_types::RelationOrSoname::Relation(package_relation) => {
            package_relation.name.to_smolstr()
        }
        alpm_types::RelationOrSoname::SonameV1(soname_v1) => {
            soname_v1.shared_object_name().to_smolstr()
        }
        alpm_types::RelationOrSoname::SonameV2(soname_v2) => soname_v2.soname.name.to_smolstr(),
    };

    Ok(Description {
        name: name.inner().into(),
        dependencies: dependencies.into_iter().map(transform).collect(),
        provides: provides.into_iter().map(transform).collect(),
        file: file_name.to_smolstr(),
        repo,
    })
}

#[derive(Default, Debug, IntoReport)]
#[message("could not scan packages")]
struct PackageScanError;

fn scan_project(project: &Path) -> Result<Vec<Description>, PackageScanError> {
    let mut entries = vec![];

    for entry in read_dir(project).wrap()? {
        let entry = entry.wrap()?;
        let repo = entry.file_name().into_encoded_bytes();
        let repo = String::from_utf8(repo).wrap()?.to_smolstr();

        for entry in read_dir(entry.path()).wrap()? {
            let entry = entry.wrap()?;
            let content = std::fs::read_to_string(entry.path().join("desc")).wrap()?;

            entries.push(content_to_description(&content, repo.clone()).wrap()?);
        }
    }

    Ok(entries)
}

#[derive(Debug)]
enum IndexEntryType {
    Package {
        repo: SmolStr,
        file: SmolStr,
        dependencies: Vec<SmolStr>,
    },
    Reference {
        origin: SmolStr,
    },
}

#[derive(Debug)]
struct IndexEntry {
    priority: usize,
    ty: IndexEntryType,
}

fn package_name(identifier: impl Into<SmolStr>) -> PackageName {
    PackageName {
        identifier: identifier.into(),
        namespace: ["xuehua".into(), "arch".into()].into(),
        ty: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{ArchBackend, Description, IndexEntry, IndexEntryType, Options};

    #[test]
    fn test_index_resolution() {
        let backend = ArchBackend {
            options: Options {
                mirror: Default::default(),
                architecture: Default::default(),
                repos: Default::default(),
                priorities: BTreeMap::from([("my-other-pkg".into(), 1), ("my-next-pkg".into(), 2)]),
            },
        };

        let entries = vec![
            Description {
                name: "my-pkg".into(),
                provides: vec!["my-library".into()],
                ..Default::default()
            },
            Description {
                name: "my-other-pkg".into(),
                provides: vec!["my-library".into()],
                ..Default::default()
            },
            Description {
                name: "my-next-pkg".into(),
                provides: vec!["my-other-pkg".into()],
                ..Default::default()
            },
        ];

        let index = backend.resolve_index(entries);

        match index.get("my-other-pkg") {
            Some(IndexEntry {
                priority: 2,
                ty: IndexEntryType::Reference { origin },
                ..
            }) if origin == "my-next-pkg" => (),
            _ => panic!("my-other-pkg did not resolve to the expected value"),
        }

        match index.get("my-library") {
            Some(IndexEntry {
                priority: 1,
                ty: IndexEntryType::Reference { origin },
                ..
            }) if origin == "my-other-pkg" => (),
            _ => panic!("my-library did not resolve to the expected value"),
        }
    }
}

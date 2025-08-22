pub mod decoding;

use std::{collections::HashSet, path::Path, str::FromStr};

use bytes::Bytes;
use smol_str::SmolStr;
use xh_engine::{
    backend::Backend,
    encoding::to_value,
    package::{Dependency, DispatchRequest, LinkTime, Metadata, Package, PackageName},
    planner::{Planner, Unfrozen},
};
use xh_executor_bubblewrap::CommandRequest;
use xh_executor_http::HttpRequest;
use xh_reports::prelude::*;

use crate::decoding::{Decoder, Package as AlpinePackage, PackageReference};

#[derive(Default, Debug, IntoReport)]
#[message("could not run alpine backend")]
pub struct Error;

#[derive(Debug, Clone, PartialEq)]
pub enum AlpineValue {}

pub struct AlpineBackend {
    base_url: String,
}

impl AlpineBackend {
    #[inline]
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

impl Backend for AlpineBackend {
    type Error = Error;

    type Value = AlpineValue;

    // TODO: provide the user with conflict resolution instead of ignoring seen packages
    fn plan(&self, planner: &mut Planner<Unfrozen>, project: &Path) -> Result<(), Self::Error> {
        let mut data = Bytes::from_owner(std::fs::read(project.join("APKINDEX")).wrap()?);
        let mut seen = HashSet::new();

        Decoder::decode(&mut data).try_for_each(move |package| {
            let mut package = package.wrap()?;
            if !seen.contains(&package.name) {
                seen.insert(package.name.clone());

                package
                    .provides
                    .drain(..)
                    .into_iter()
                    .try_for_each(|reference| {
                        if !seen.contains(&reference.name) {
                            seen.insert(reference.name.clone());
                            let package = self.transform_provided(package.name.clone(), reference);
                            planner.register(package).wrap()?;
                        }

                        Ok::<_, Report<_>>(())
                    })?;

                planner.register(self.transform_package(package)?).wrap()?;
            }

            Ok(())
        })
    }
}

impl AlpineBackend {
    fn transform_provided(&self, owner: SmolStr, reference: PackageReference) -> Package {
        Package {
            name: package_name(reference.name),
            metadata: Metadata,
            requests: vec![],
            dependencies: vec![Dependency {
                name: package_name(owner),
                time: LinkTime::Runtime,
            }],
        }
    }

    fn transform_package(&self, package: AlpinePackage) -> Result<Package, Error> {
        let download = HttpRequest {
            path: "package.apk".into(),
            url: FromStr::from_str(&format!(
                "{}/{}-{}.apk",
                self.base_url, package.name, package.version
            ))
            .wrap()?,
            method: FromStr::from_str("GET").expect("GET should be valid method"),
        };

        let unpack = CommandRequest {
            program: "/busybox".into(),
            arguments: ["tar", "-xf", "package.apk", "-C", "output"]
                .into_iter()
                .map(Into::into)
                .collect(),
            ..Default::default()
        };

        let requests = vec![
            DispatchRequest {
                executor: "http@xuehua/executors".into(),
                payload: to_value(download).wrap()?,
            },
            DispatchRequest {
                executor: "bubblewrap@xuehua/executors".into(),
                payload: to_value(unpack).wrap()?,
            },
        ];

        let package = Package {
            name: package_name(package.name),
            metadata: Metadata,
            requests: requests.clone(),
            dependencies: package
                .dependencies
                .into_iter()
                .map(|dependency| Dependency {
                    name: package_name(dependency.name),
                    time: LinkTime::Runtime,
                })
                .collect(),
        };

        Ok(package)
    }
}

fn package_name(name: SmolStr) -> PackageName {
    PackageName {
        identifier: name,
        namespace: vec!["alpine".into()],
    }
}

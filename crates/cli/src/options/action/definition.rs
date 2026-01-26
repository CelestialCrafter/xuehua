
use xh_engine::name::PackageName;

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

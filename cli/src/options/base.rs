use std::fs::create_dir_all;
use std::path::Path;
use std::{fs, path::PathBuf};

use dirs::{config_dir, data_dir, runtime_dir};
use eyre::Result;
use log::{debug, info, warn};
use tempfile::env::temp_dir;

const BUILD: &str = "xuehua/builds";
const STORE: &str = "xuehua/store";
const OPTIONS: &str = "xuehua/options.toml";

pub struct Locations {
    pub build: PathBuf,
    pub store: PathBuf,
    pub options: PathBuf,
}

fn user_locations() -> Option<Locations> {
    Some(Locations {
        build: runtime_dir()?.join(BUILD),
        store: data_dir()?.join(STORE),
        options: config_dir()?.join(OPTIONS),
    })
}

fn system_locations() -> Locations {
    Locations {
        build: temp_dir().join(BUILD),
        #[cfg(unix)]
        store: Path::new("/var/lib").join(STORE),
        #[cfg(unix)]
        options: Path::new("/etc").join(OPTIONS),
    }
}

enum LocationTypes {
    User,
    System,
}

fn initialize_locations() -> Result<Locations> {
    let user = user_locations();
    let system = system_locations();

    let loc_ty = [
        (LocationTypes::User, user.as_ref()),
        (LocationTypes::System, Some(&system)),
    ]
    .into_iter()
    .find_map(|(loc_ty, loc)| {
        let path = &loc?.options;
        debug!("searching path: {:?}", path);

        fs::exists(path)
            .inspect_err(|err| warn!("could not check if options file at {path:?} exists: {err}"))
            .ok()?
            .then_some(loc_ty)
    })
    .unwrap_or_else(|| {
        let (name, loc_ty) = if user.is_some() {
            ("user", LocationTypes::User)
        } else {
            ("system", LocationTypes::System)
        };

        info!("could not find options file, falling back to {name} locations");
        loc_ty
    });

    let preset = match loc_ty {
        LocationTypes::User => user.unwrap(),
        LocationTypes::System => system,
    };

    create_dir_all(&preset.build)?;
    create_dir_all(&preset.store)?;

    Ok(preset)
}

pub struct BaseOptions {
    pub locations: Locations,
}

impl BaseOptions {
    pub fn run() -> Result<Self> {
        Ok(Self {
            locations: initialize_locations()?,
        })
    }
}

use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use dirs::{config_dir, data_dir, runtime_dir};
use log::{info, warn};
use tempfile::env::temp_dir;
use xh_reports::{compat::StdCompat, prelude::*};

const BUILD: &str = "xuehua/builds";
const STORE: &str = "xuehua/store";
const OPTIONS: &str = "xuehua/options.toml";

#[derive(Debug, Clone)]
pub struct Locations {
    pub build: PathBuf,
    pub store: PathBuf,
    pub options: PathBuf,
}

enum LocationType {
    User,
    System,
}

impl Display for LocationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LocationType::User => "user",
            LocationType::System => "system",
        })
    }
}

#[derive(Debug, IntoReport)]
#[message("could not initialize locations")]
#[context(locations)]
pub struct InitializeLocationsError {
    locations: Locations,
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

fn initialize_locations() -> Result<Locations, InitializeLocationsError> {
    let system = system_locations();
    let user = user_locations();

    if let None = user {
        warn!(
            suggestion = "ensure that the XDG_RUNTIME_DIR, XDG_DATA_HOME, and XDG_CONFIG_HOME environment variables are set";
            "could not evaluate user locations"
        );
    }

    let ty = [
        (LocationType::System, Some(&system)),
        (LocationType::User, user.as_ref()),
    ]
    .into_iter()
    .find_map(|(ty, preset)| {
        let locations = preset?;
        let path = &locations.options;

        match fs::exists(path) {
            Ok(true) => return Some(ty),
            Ok(false) => (),
            Err(err) => warn!(
                error:err = err;
                "could not check if options file at {path:?} exists"
            ),
        }

        None
    })
    .unwrap_or_else(|| {
        let (name, path, ty) = if let Some(ref user) = user {
            ("user", &user.options, LocationType::User)
        } else {
            ("system", &system.options, LocationType::System)
        };

        info!(
            suggestion = format!("create a config file at {}", path.display());
            "could not find options file, falling back to {name} locations"
        );

        ty
    });

    let preset = match ty {
        LocationType::User => user.unwrap(),
        LocationType::System => system,
    };

    fs::create_dir_all(&preset.build)
        .and_then(|()| fs::create_dir_all(&preset.store))
        .compat()
        .wrap_with_fn(|| InitializeLocationsError {
            locations: preset.clone(),
        })
        .map(|()| preset)
}

#[derive(Default, Debug, IntoReport)]
#[message("could not create base options")]
pub struct Error;

pub struct BaseOptions {
    pub locations: Locations,
}

impl BaseOptions {
    pub fn read() -> Result<Self, Error> {
        Ok(Self {
            locations: initialize_locations().wrap()?,
        })
    }
}

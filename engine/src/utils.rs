pub mod passthru;

use std::{fs, io, path::Path};

use mlua::{AnyUserData, Function, Lua, MetaMethod, Table, chunk};

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

#[macro_export]
macro_rules! impl_into_err {
    ($(($error:ty, $fn:ident)),*) => {
        /// Trait for converting [`std::result::Result`] into Lua [`Result`].
        pub trait ExternalResult<T> {
            $(fn $fn(self) -> Result<T, $error>;)*
        }

        pub trait ExternalError {
            $(fn $fn(self) -> $error;)*
        }

        impl<T, E: Into<crate::utils::BoxDynError>> ExternalResult<T> for Result<T, E>
        {
            $(fn $fn(self) -> Result<T, $error> {
                self.map_err(|err| err.$fn())
            })*
        }

        impl<E: Into<crate::utils::BoxDynError>> ExternalError for E
        {
            $(fn $fn(self) -> $error {
                <$error>::ExternalError(self.into())
            })*
        }
    };
}

pub fn register_local_module(
    lua: &Lua,
    name: &str,
    table: &'static str,
) -> Result<(), mlua::Error> {
    let metatable = lua.create_table()?;
    metatable.set(
        MetaMethod::Index.name(),
        lua.create_function(move |lua, (_, executor): (Table, String)| {
            lua.globals()
                .get::<Table>(table)?
                .get::<AnyUserData>(executor)
        })?,
    )?;

    let table = lua.create_table()?;
    table.set_metatable(Some(metatable))?;

    lua.register_module(name, table)?;

    Ok(())
}

pub fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(_) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn register_module(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;

    let [runtime, buildtime, no_config] = lua
        .load(chunk! {
            local function runtime(pkg)
                return { type = "runtime", package = pkg }
            end

            local function buildtime(pkg)
                return { type = "buildtime", package = pkg }
            end

            local function no_config(pkg)
                pkg.defaults = {}
                pkg.configure = function(_)
                    return pkg
                end

                return pkg
            end

            return { runtime, buildtime, no_config }
        })
        .eval::<[Function; 3]>()
        .expect("util functions should evaluate");

    module.set("runtime", runtime)?;
    module.set("buildtime", buildtime)?;
    module.set("no_config", no_config)?;

    lua.register_module("xuehua.utils", module)?;

    Ok(())
}

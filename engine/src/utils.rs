use std::{fs, io, path::Path};

use mlua::{Function, Lua, chunk};

#[macro_export]
macro_rules! impl_into_err {
    ($(($error:ty, $fn:ident)),*) => {
        /// Trait for converting [`std::result::Result`] into Lua [`Result`].
        pub trait ExternalResult<T> {
            $(fn $fn(self) -> Result<T, $error>; )*
        }

        impl<T, E: Into<Box<dyn std::error::Error + Send + Sync>>> ExternalResult<T> for Result<T, E>
        {
            $(fn $fn(self) -> Result<T, $error> {
                self.map_err(|err| <$error>::ExternalError(err.into()))
            })*
        }
    };
}

pub fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(_) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn inject(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;

    let [runtime, buildtime, no_config] = lua
        .load(chunk! {
            local function runtime(id)
                return { id, "runtime" }
            end

            local function buildtime(id)
                return { id, "buildtime" }
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

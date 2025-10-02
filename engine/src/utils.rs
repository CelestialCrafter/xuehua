use std::{fs, io, path::Path};

use mlua::{Function, Lua, chunk};

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

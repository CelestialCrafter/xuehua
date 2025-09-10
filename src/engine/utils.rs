use mlua::{chunk, Function, Lua};

pub fn inject(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;

    module.set(
        "buildtime",
        lua.load(chunk! {
            function(package)
                return { id = "package", type = "build" }
            end
        })
        .eval::<Function>()
        .expect("buildtime function should evaluate"),
    )?;

    module.set(
        "runtime",
        lua.load(chunk! {
            function(package)
                return { id = "package", type = "run" }
            end
        })
        .eval::<Function>()
        .expect("runtime function should evaluate"),
    )?;

    lua.register_module("xuehua.utils", module)?;

    Ok(())
}

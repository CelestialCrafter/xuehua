use mlua::{Function, Lua, chunk};

pub fn inject(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;

    module.set(
        "buildtime",
        lua.load(chunk! {
            function(id)
                return { id, "buildtime" }
            end
        })
        .eval::<Function>()
        .expect("buildtime function should evaluate"),
    )?;

    module.set(
        "runtime",
        lua.load(chunk! {
            function(id)
                return { id, "runtime" }
            end
        })
        .eval::<Function>()
        .expect("runtime function should evaluate"),
    )?;

    lua.register_module("xuehua.utils", module)?;

    Ok(())
}

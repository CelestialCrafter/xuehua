use mlua::Lua;
use tracing::{Level, event};

macro_rules! add_level {
    ($level:expr, $lua:expr) => {
        $lua.create_function(move |_, message: String| {
            event!(
                target: concat!(module_path!(), "runtime"),
                $level,
                message
            );

            Ok(())
        })?
    };
}

pub fn register_module(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;
    module.set("info", add_level!(Level::INFO, lua))?;
    module.set("warn", add_level!(Level::WARN, lua))?;
    module.set("error", add_level!(Level::ERROR, lua))?;
    module.set("debug", add_level!(Level::DEBUG, lua))?;
    module.set("trace", add_level!(Level::TRACE, lua))?;
    lua.register_module("xuehua.logger", module)?;

    Ok(())
}

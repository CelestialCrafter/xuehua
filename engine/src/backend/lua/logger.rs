use std::str::FromStr;

use log::{Level, Record, logger};
use mlua::{ExternalResult, Lua};

fn log(level: Level, message: String) {
    logger().log(
        &Record::builder()
            .level(level)
            .target("xh_engine::lua")
            .args(format_args!("{}", message))
            .build(),
    );
}

pub fn register_module(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;
    let add_level = |name, level| {
        module.set(
            name,
            lua.create_function(move |_, message| Ok(log(level, message)))?,
        )
    };

    add_level("info", Level::Info)?;
    add_level("warn", Level::Warn)?;
    add_level("error", Level::Error)?;
    add_level("debug", Level::Debug)?;
    add_level("trace", Level::Trace)?;
    module.set(
        "log",
        lua.create_function(move |_, (level, message): (String, _)| {
            Ok(log(
                Level::from_str(level.as_str()).into_lua_err()?,
                message,
            ))
        })?,
    )?;

    lua.register_module("xuehua.logger", module)?;

    Ok(())
}

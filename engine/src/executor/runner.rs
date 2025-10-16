#[cfg(feature = "bubblewrap-executor")]
pub mod bubblewrap;
#[cfg(feature = "bubblewrap-executor")]
pub use bubblewrap::BubblewrapExecutor;

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    process::Command,
};

use mlua::UserData;

pub struct LuaCommand(pub Command);

impl LuaCommand {
    fn new(program: &OsStr) -> Self {
        Self(Command::new(program))
    }
}

impl UserData for LuaCommand {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // arguments
        fields.add_field_method_set("arguments", |_, this, args: Vec<OsString>| {
            this.0.args(args);
            Ok(())
        });
        fields.add_field_method_get("arguments", |_, this| {
            Ok(this
                .0
                .get_args()
                .map(|v| v.to_os_string())
                .collect::<Vec<_>>())
        });

        // environment
        fields.add_field_method_set(
            "environment",
            |_, this, envs: HashMap<OsString, OsString>| {
                this.0.envs(envs);
                Ok(())
            },
        );
        fields.add_field_method_get("environment", |_, this| {
            Ok(this
                .0
                .get_envs()
                .map(|(k, v)| (k.to_os_string(), v.map(|v| v.to_os_string())))
                .collect::<HashMap<_, _>>())
        });

        // working dir
        fields.add_field_method_set("working_dir", |_, this, dir: OsString| {
            this.0.current_dir(dir);
            Ok(())
        });
        fields.add_field_method_get("working_dir", |_, this| {
            Ok(this.0.get_current_dir().map(|p| p.to_path_buf()))
        });
    }
}

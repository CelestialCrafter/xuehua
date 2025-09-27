#[cfg(feature = "bubblewrap-builder")]
pub mod bubblewrap;

use std::{
    collections::HashMap,
    ffi::OsString,
    io,
    path::Path,
    process::{Command, Output},
    string::FromUtf8Error,
};

use mlua::{FromLua, MetaMethod, UserData, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BuilderError {
    #[cfg(feature = "bubblewrap-builder")]
    #[error(transparent)]
    JSONSerializationError(#[from] serde_json::Error),
    #[error("builder is uninitialized")]
    Uninitialized,
    #[error(transparent)]
    IOError(#[from] io::Error),
}

pub struct LuaCommand(pub Command);

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

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(
            MetaMethod::Call,
            |lua, (_proxy, program): (Value, Value)| {
                let program = OsString::from_lua(program, lua)?;
                Ok(Self(Command::new(program)))
            },
        );
    }
}

pub struct LuaOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

impl TryFrom<Output> for LuaOutput {
    type Error = FromUtf8Error;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        Ok(Self {
            exit_code: value.status.code(),
            stdout: String::from_utf8(value.stdout)?,
            stderr: String::from_utf8(value.stderr)?,
        })
    }
}

impl UserData for LuaOutput {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("stdout", |_, this| Ok(this.stdout.clone()));
        fields.add_field_method_get("stderr", |_, this| Ok(this.stderr.clone()));
        fields.add_field_method_get("exit_code", |_, this| Ok(this.exit_code));
    }
}

pub trait Builder: Sized {
    fn init(&mut self, dependencies: Vec<&Path>) -> Result<(), BuilderError>;
    fn run(&mut self, command: &Command) -> Result<Output, BuilderError>;
    fn output(&self) -> &Path;
}

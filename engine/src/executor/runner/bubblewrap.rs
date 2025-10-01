use std::{
    ffi::{OsStr, OsString},
    io,
    path::PathBuf,
    process::Command,
};

use mlua::{AnyUserData, FromLuaMulti, IntoLuaMulti, Lua, MultiValue, Value};

use crate::{
    ExternalResult,
    executor::{Error, Executor, LuaCommand},
};

#[derive(Debug)]
pub struct BubblewrapExecutorOptions {
    network: bool,
    add_capabilities: Vec<String>,
    drop_capabilities: Vec<String>,
}

impl Default for BubblewrapExecutorOptions {
    fn default() -> Self {
        Self {
            network: true,
            add_capabilities: Default::default(),
            drop_capabilities: Default::default(),
        }
    }
}

/// A command executor using [`bubblewrap`](https://github.com/containers/bubblewrap) for sandboxing
///
/// # Security/Sandboxing
///
/// This executor attempts to enforce a security boundary to ensure reproducability
/// and minimize the impact of malicious build scripts on the host system.
///
// NOTE: ensure this list stays in sync with the code
/// By default, the following safety related `bubblewrap` flags are enabled by default:
/// - `--new-session`
/// - `--unshare-all`
/// - `--clearenv`
///
/// # Command Runner
///
/// To execute multiple commands within the sandbox, this executor bundles a command runner.
/// The runner is embedded within the library at compile-time, and is controlled via stdin/stdout.
pub struct BubblewrapExecutor {
    environment: PathBuf,
    options: BubblewrapExecutorOptions,
}

impl BubblewrapExecutor {
    pub fn new(environment: PathBuf, options: BubblewrapExecutorOptions) -> Result<Self, Error> {
        Ok(Self {
            environment,
            options,
        })
    }
}

impl Executor for BubblewrapExecutor {
    fn create(&self, lua: &Lua, value: MultiValue) -> Result<AnyUserData, Error> {
        let (program,) = <(OsString,)>::from_lua_multi(value, lua)?;
        let userdata = lua.create_userdata(LuaCommand::new(&program))?;

        Ok(userdata)
    }

    fn dispatch(&mut self, lua: &Lua, data: AnyUserData) -> Result<MultiValue, Error> {
        let mut command = Command::new("bwrap");

        // essentials
        command
            .arg("--bind")
            .arg(&self.environment)
            .arg("/")
            .args(&[
                // TODO: remove busybox
                "--ro-bind",
                "busybox.static",
                "/busybox",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
            ]);

        // restrictions
        command.args([
            "--new-session",
            "--die-with-parent",
            "--clearenv",
            "--unshare-all",
        ]);

        command.args(
            self.options
                .add_capabilities
                .iter()
                .map(|cap| ["--cap-add", cap])
                .flatten(),
        );

        command.args(
            self.options
                .drop_capabilities
                .iter()
                .map(|cap| ["--cap-drop", cap])
                .flatten(),
        );

        if self.options.network {
            command.arg("--share-net");
        }

        // command payload
        let lua_command = data.take::<LuaCommand>()?.0;

        if let Some(working_dir) = lua_command.get_current_dir() {
            command.arg("--chdir").arg(working_dir);
        }

        command.args(
            lua_command
                .get_envs()
                .into_iter()
                .filter_map(|(key, value)| Some([OsStr::new("--setenv"), key, value?]))
                .flatten(),
        );

        command
            .arg("--")
            .arg(lua_command.get_program())
            .args(lua_command.get_args());

        // execution
        let output = command.output().into_executor_err()?;
        let stdout = String::from_utf8(output.stdout).into_executor_err()?;
        let stderr = String::from_utf8(output.stderr).into_executor_err()?;

        if !output.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{}\nstderr: {}", output.status, stderr),
            ))
            .into_executor_err();
        }

        let table = lua.create_table()?;
        table.set("status", output.status.code().unwrap_or(-1))?;
        table.set("stdout", stdout)?;
        table.set("stderr", stderr)?;

        Ok(Value::Table(table).into_lua_multi(lua)?)
    }
}

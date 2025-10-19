use std::{
    ffi::{OsStr, OsString},
    io,
    path::PathBuf, process::Command,
};

use futures_util::{FutureExt, future::BoxFuture};
use mlua::{AnyUserData, FromLuaMulti, IntoLuaMulti, Lua, MultiValue, Value};
use tokio::process::Command as TokioCommand;

use crate::{
    ExternalResult,
    executor::{Error, Executor, runner::LuaCommand},
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
    pub fn new(environment: PathBuf, options: BubblewrapExecutorOptions) -> Self {
        Self {
            environment,
            options,
        }
    }

    async fn dispatch_impl(&mut self, lua: Lua, data: AnyUserData) -> Result<MultiValue, Error> {
        let command = Command::new("bwrap");
        let mut command = command;

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
        let output = TokioCommand::from(command)
            .output()
            .await
            .into_executor_err()?;
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

        Ok(Value::Table(table).into_lua_multi(&lua)?)
    }
}

impl Executor for BubblewrapExecutor {
    fn create(&self, lua: &Lua, value: MultiValue) -> Result<AnyUserData, Error> {
        let (program,) = <(OsString,)>::from_lua_multi(value, lua)?;
        let userdata = lua.create_userdata(LuaCommand::new(&program))?;

        Ok(userdata)
    }

    fn dispatch(
        &'_ mut self,
        lua: Lua,
        data: AnyUserData,
    ) -> BoxFuture<'_, Result<MultiValue, Error>> {
        self.dispatch_impl(lua, data).boxed()
    }
}

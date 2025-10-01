use std::{ffi::OsStr, io, iter::once, path::Path, process::Command, sync::Arc};

use log::debug;
use tokio::process::Command as TokioCommand;

use crate::{executor::{
    runner::{LuaCommand, LuaOutput}, Executor
}, utils::BoxDynError};

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
    environment: Arc<Path>,
    options: BubblewrapExecutorOptions,
}

impl BubblewrapExecutor {
    pub fn new(environment: Arc<Path>, options: BubblewrapExecutorOptions) -> Self {
        Self {
            environment,
            options,
        }
    }
}

impl Executor for BubblewrapExecutor {
    type Request = LuaCommand;
    type Response = LuaOutput;

    async fn dispatch(&mut self, request: Self::Request) -> Result<Self::Response, BoxDynError> {
        let original = request.0;
        debug!(
            "running command {:?}",
            once(original.get_program())
                .chain(original.get_args())
                .collect::<Vec<_>>()
                .join(OsStr::new(" ")),
        );

        let mut sandboxed = Command::new("bwrap");

        // essentials
        sandboxed
            .arg("--bind")
            .arg(&*self.environment)
            .arg("/")
            .args(&[
                // TODO: move busybox bootstrap to its own package
                "--ro-bind",
                "busybox-bootstrap",
                "/busybox",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
            ]);

        // restrictions
        sandboxed.args([
            "--new-session",
            "--die-with-parent",
            "--clearenv",
            "--unshare-all",
        ]);

        sandboxed.args(
            self.options
                .add_capabilities
                .iter()
                .map(|cap| ["--cap-add", cap])
                .flatten(),
        );

        sandboxed.args(
            self.options
                .drop_capabilities
                .iter()
                .map(|cap| ["--cap-drop", cap])
                .flatten(),
        );

        if self.options.network {
            sandboxed.arg("--share-net");
        }

        // command payload
        if let Some(working_dir) = original.get_current_dir() {
            sandboxed.arg("--chdir").arg(working_dir);
        }

        sandboxed.args(
            original
                .get_envs()
                .into_iter()
                .filter_map(|(key, value)| Some([OsStr::new("--setenv"), key, value?]))
                .flatten(),
        );

        sandboxed
            .arg("--")
            .arg(original.get_program())
            .args(original.get_args());

        // execution
        let output = TokioCommand::from(sandboxed)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8(output.stderr)?;
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{}\nstderr: {}", output.status, stderr),
            ).into());
        }

        Ok(LuaOutput::try_from(output)?)
    }
}

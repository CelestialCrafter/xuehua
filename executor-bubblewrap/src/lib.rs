use std::sync::Arc;

use log::debug;
use serde::Deserialize;
use thiserror::Error;

use xh_engine::{builder::InitializeContext, executor::Executor};

#[derive(Error, Debug)]
pub enum Error {
    // TODO: improve this error
    #[error("command exited with code {0:?}")]
    CommandFailed(std::process::ExitStatus),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
pub struct CommandRequest {
    program: String,
    working_dir: Option<String>,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Options {
    network: bool,
    add_capabilities: Vec<String>,
    drop_capabilities: Vec<String>,
}

impl Default for Options {
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
    ctx: Arc<InitializeContext>,
    options: Options,
}

impl BubblewrapExecutor {
    pub fn new(ctx: Arc<InitializeContext>, options: Options) -> Self {
        Self { ctx, options }
    }
}

impl Executor for BubblewrapExecutor {
    const NAME: &'static str = "bubblewrap@xuehua/executors";
    type Request = CommandRequest;
    type Error = Error;

    async fn execute(&mut self, request: Self::Request) -> Result<(), Self::Error> {
        debug!(
            "running command {:?}",
            std::iter::once(request.program.clone())
                .chain(request.arguments.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );

        let mut sandboxed = tokio::process::Command::new("bwrap");

        // essentials
        sandboxed
            .arg("--bind")
            .arg(&self.ctx.environment)
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
        if let Some(working_dir) = request.working_dir {
            sandboxed.arg("--chdir").arg(working_dir);
        }

        for (key, value) in request.environment {
            sandboxed.args(["--setenv", &key, &value]);
        }

        sandboxed
            .arg("--")
            .arg(request.program)
            .args(request.arguments);

        let status = sandboxed.spawn()?.wait().await?;
        status
            .success()
            .then_some(())
            .ok_or(Error::CommandFailed(status))
    }
}

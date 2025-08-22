use std::{
    process::{ExitStatus, Output},
    sync::Arc,
};

use log::debug;
use serde::{Deserialize, Serialize};

use smol_str::SmolStr;
use xh_engine::{builder::InitializeContext, executor::Executor};
use xh_reports::prelude::*;

#[derive(Debug, IntoReport)]
#[message("external command failed")]
#[context(status)]
#[attachment(stderr)]
pub struct CommandError {
    status: ExitStatus,
    stderr: String,
}

#[derive(Default, Debug, IntoReport)]
#[message("could not execute request")]
pub struct Error;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandRequest {
    pub program: SmolStr,
    pub working_dir: Option<SmolStr>,
    pub arguments: Vec<SmolStr>,
    pub environment: Vec<(SmolStr, SmolStr)>,
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
    #[inline]
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
            .args([
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
                .flat_map(|cap| ["--cap-add", cap]),
        );

        sandboxed.args(
            self.options
                .drop_capabilities
                .iter()
                .flat_map(|cap| ["--cap-drop", cap]),
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

        let Output {
            status,
            stderr,
            stdout: _,
        } = sandboxed.output().await.wrap()?;
        status
            .success()
            .then_some(())
            .ok_or(CommandError {
                status,
                stderr: String::from_utf8_lossy(&stderr).to_string(),
            })
            .wrap()
    }
}

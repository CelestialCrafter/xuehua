use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader, Seek, Write},
    mem::forget,
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process,
};

use log::warn;
use once_cell::sync::OnceCell;
use rustix::io::dup2;
use serde::{Deserialize, Serialize};
use tempfile::tempfile;

use crate::executor::{Executor, Error};

static PARENT_FD: OnceCell<OwnedFd> = OnceCell::new();
const CHILD_FD: i32 = 10;

#[derive(Serialize, Debug)]
struct CommandRequest {
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    environment: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct CommandResponseInfo {
    exit_code: i32,
    stderr: Vec<u8>,
    stdout: Vec<u8>,
}

#[derive(Deserialize, Debug)]
struct CommandResponse {
    info: Option<CommandResponseInfo>,
    error: Option<String>,
}

impl CommandResponse {
    fn extract(self) -> Result<CommandResponseInfo, Error> {
        self.info
            .ok_or_else(|| self.error.unwrap_or("no error or info set".to_string()))
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
            .map_err(Error::from)
    }
}

struct InitData {
    child: process::Child,
    stdin: process::ChildStdin,
    stdout: BufReader<process::ChildStdout>,
}

#[derive(Default, Debug)]
pub struct BubblewrapExecutorOptions {
    network: bool,
    add_capabilities: Vec<String>,
    drop_capabilities: Vec<String>,
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
    options: BubblewrapExecutorOptions,
    output: PathBuf,
    init: Option<InitData>,
}

impl BubblewrapExecutor {
    pub fn new(output: PathBuf, options: BubblewrapExecutorOptions) -> Self {
        Self {
            init: None,
            output,
            options,
        }
    }
}

impl Executor for BubblewrapExecutor {
    fn init(&mut self, dependencies: Vec<&Path>) -> Result<(), Error> {
        let mut command = process::Command::new("bwrap");
        // dependencies
        if !dependencies.is_empty() {
            command
                .args(
                    dependencies
                        .into_iter()
                        .map(|path| [OsStr::new("--overlay-src"), path.as_os_str()])
                        .flatten(),
                )
                .args(&["--tmp-overlay", "/"]);
        }

        // essentials
        command
            .args(&[
                "--ro-bind",
                "busybox.static",
                "/busybox",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--bind",
            ])
            .arg(&self.output)
            .arg("/output");

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

        // cmd runner
        command.args(&[
            "--perms",
            "0744",
            "--ro-bind-data",
            &CHILD_FD.to_string(),
            "/shell",
        ]);

        unsafe {
            command.pre_exec(|| {
                let mut fd = OwnedFd::from_raw_fd(CHILD_FD);
                dup2(
                    PARENT_FD.get_or_try_init(|| {
                        let mut file = tempfile()?;
                        file.write_all(include_bytes!(concat!(env!("OUT_DIR"), "/cmd-runner")))?;
                        file.rewind()?;
                        Ok::<_, io::Error>(OwnedFd::from(file))
                    })?,
                    &mut fd,
                )?;
                forget(fd);

                Ok(())
            });
        };

        // execution
        command
            .args(&["--", "/shell"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("should be able to take stdin");
        let stdout = BufReader::new(child.stdout.take().expect("should be able to take stdout"));
        self.init = Some(InitData {
            child,
            stdin,
            stdout,
        });

        Ok(())
    }

    fn run(&mut self, command: &process::Command) -> Result<process::Output, Error> {
        let child = self.init.as_mut().ok_or(Error::Uninitialized)?;
        // weird workaround so the compiler doesnt yell at me
        let (stdin, stdout) = (&mut child.stdin, &mut child.stdout);

        let to_string = |os_str: &OsStr| {
            os_str.to_os_string().into_string().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "non utf-8 data in command")
            })
        };

        serde_json::to_writer(
            stdin,
            &CommandRequest {
                program: to_string(command.get_program())?,
                args: command
                    .get_args()
                    .map(to_string)
                    .collect::<Result<_, _>>()?,
                working_dir: command.get_current_dir().map(|v| v.to_path_buf()),
                environment: command
                    .get_envs()
                    .map(|(k, v)| {
                        Ok::<_, io::Error>(format!(
                            "{}={}",
                            to_string(k)?,
                            to_string(v.unwrap_or_default())?
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            },
        )?;

        let buf = &mut String::with_capacity(8192);
        stdout.read_line(buf)?;
        let response = serde_json::from_reader::<_, CommandResponse>(buf.as_bytes())?.extract()?;
        Ok(process::Output {
            status: ExitStatusExt::from_raw(response.exit_code),
            stdout: response.stdout,
            stderr: response.stderr,
        })
    }

    fn output(&self) -> &Path {
        &self.output
    }
}

impl Drop for BubblewrapExecutor {
    fn drop(&mut self) {
        if let Some(InitData { ref mut child, .. }) = self.init {
            if let Err(err) = child.kill() {
                warn!(err:? = err; "could not kill child");
            }
        }
    }
}

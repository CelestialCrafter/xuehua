use std::{
    ffi::OsStr,
    fs::{self, Permissions},
    io::{self, BufRead, BufReader},
    os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio},
    sync::LazyLock,
};

use blake3::{Hash, hash};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::{
    TEMP_DIR,
    modules::builder::{Builder, BuilderError},
};

const CMD_RUNNER_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cmd-runner"));
static CMD_RUNNER_HASH: LazyLock<Hash> = LazyLock::new(|| hash(CMD_RUNNER_BIN));

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
    fn extract(self) -> Result<CommandResponseInfo, BuilderError> {
        self.info
            .ok_or_else(|| self.error.unwrap_or("no error or info set".to_string()))
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
            .map_err(BuilderError::from)
    }
}

pub struct BubblewrapBuilder {
    output: PathBuf,
    child: Option<(Child, ChildStdin, BufReader<ChildStdout>)>,
}

impl BubblewrapBuilder {
    pub fn new(output: PathBuf) -> Self {
        Self {
            child: None,
            output,
        }
    }
}

impl Builder for BubblewrapBuilder {
    fn init(&mut self, dependencies: Vec<&Path>) -> Result<(), BuilderError> {
        let cmd_runner_path = TEMP_DIR.join(format!("cmd-runner-{}", CMD_RUNNER_HASH.to_hex()));
        if !fs::exists(&cmd_runner_path)? {
            fs::write(&cmd_runner_path, CMD_RUNNER_BIN)?;
            fs::set_permissions(&cmd_runner_path, Permissions::from_mode(0744))?;
        }

        fs::create_dir(&self.output)?;
        let mut command = Command::new("bwrap");
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

        // cmd runner
        command
            .args(&["--ro-bind"])
            .arg(cmd_runner_path)
            .arg("/shell");

        // execution
        command
            .args(&["--", "/shell"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("should be able to take stdin");
        let stdout = BufReader::new(child.stdout.take().expect("should be able to take stdout"));
        self.child = Some((child, stdin, stdout));

        Ok(())
    }

    fn run(&mut self, command: &Command) -> Result<Output, BuilderError> {
        let child = self.child.as_mut().ok_or(BuilderError::Uninitialized)?;
        // weird workaround so the compiler doesnt yell at me
        let (stdin, stdout) = (&mut child.1, &mut child.2);

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
        Ok(Output {
            status: ExitStatusExt::from_raw(response.exit_code),
            stdout: response.stdout,
            stderr: response.stderr,
        })
    }

    fn output(&self) -> &Path {
        &self.output
    }
}

impl Drop for BubblewrapBuilder {
    fn drop(&mut self) {
        if let Some((ref mut child, _, _)) = self.child {
            if let Err(err) = child.kill() {
                warn!(err:? = err; "could not kill child");
            }
        }
    }
}

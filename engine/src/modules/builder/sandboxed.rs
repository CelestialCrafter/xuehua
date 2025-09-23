use std::{
    ffi::OsStr,
    fs,
    io::{self, BufRead, BufReader},
    os::unix::process::ExitStatusExt,
    path::PathBuf,
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

pub struct SandboxedBuilder {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl SandboxedBuilder {
    pub fn new() -> Result<Self, BuilderError> {
        let cmd_runner_path = TEMP_DIR.join(format!("cmd-runner-{}", CMD_RUNNER_HASH.to_hex()));
        if !fs::exists(&cmd_runner_path)? {
            fs::write(&cmd_runner_path, CMD_RUNNER_BIN)?;
        }

        // TODO: add --overlay-src <store_path>
        let mut child = Command::new("bwrap")
            // setup
            .args(&["--tmp-overlay", "/", "--proc", "/proc", "--dev", "/dev"])
            // runner
            .args(&["--perms", "0700", "--ro-bind"])
            .arg(cmd_runner_path)
            .arg("/shell")
            // execution
            .args(&["--", "/shell"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().expect("should be able to take stdin");
        let stdout = BufReader::new(child.stdout.take().expect("should be able to take stdout"));

        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }
}

impl Builder for SandboxedBuilder {
    fn run(&mut self, command: &Command) -> Result<Output, BuilderError> {
        let to_string = |os_str: &OsStr| {
            os_str.to_os_string().into_string().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "non utf-8 data in command")
            })
        };

        serde_json::to_writer(
            &mut self.stdin,
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
        self.stdout.read_line(buf)?;
        let response = serde_json::from_reader::<_, CommandResponse>(buf.as_bytes())?.extract()?;
        Ok(Output {
            status: ExitStatusExt::from_raw(response.exit_code),
            stdout: response.stdout,
            stderr: response.stderr,
        })
    }
}

impl Drop for SandboxedBuilder {
    fn drop(&mut self) {
        if let Err(err) = self.child.kill() {
            warn!(err:? = err; "could not kill child");
        }
    }
}

#[cfg(test)]
mod test {
    use std::process::Command;

    use super::SandboxedBuilder;
    use crate::modules::builder::Builder;

    #[test]
    fn test_builder() {
        let mut builder = SandboxedBuilder::new().expect("new builder should not fail");

        let input = "hii";
        let output = builder
            .run(Command::new("/busybox").args(&["sh", "-c", &format!("printf {input}")]))
            .expect("command should not fail");
        assert!(output.status.success(), "command was not successful");

        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout, input, "command output did not equal input");
    }
}

use eyre::{Context, eyre};
use mlua::AsChunk;
use std::{iter, path::Path};
use tempfile::tempdir;

use crate::{evaluator::EvaluationError, package::Package};

use super::Dependencies;

fn setup_sandbox_root(_root: &Path, _dependencies: &Dependencies) -> Result<(), EvaluationError> {
    Ok(())
}

pub fn build_package(source: impl AsChunk) -> Result<(), EvaluationError> {
    let pkg = Package {
        dependencies: Dependencies::default(),
        instructions: Vec::default(),
    };

    let root = tempdir().map_err(|err| EvaluationError::Other(err.into()))?;
    setup_sandbox_root(root.path(), &pkg.dependencies)?;

    let cmd = pkg.instructions;
    let args = {
        let bwrap = [];

        bwrap
            .into_iter()
            .chain(iter::once("--"))
            .chain(cmd.iter().map(|v| v.as_str()))
    };

    let output = duct::cmd("bwrap", args)
        .stderr_capture()
        .unchecked()
        .run()
        .wrap_err("could not run instructions")
        .map_err(EvaluationError::InstructionError)?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);

        // TODO: truncate stderr, gate full stderr behind log level
        return Err(EvaluationError::InstructionError(
            eyre!("instruction \"{cmd:?}\" returned non-zero exit code: {code}")
                .wrap_err(String::from_utf8_lossy(&output.stderr).into_owned()),
        ));
    }

    Ok(())
}

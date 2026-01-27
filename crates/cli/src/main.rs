pub mod archive;
pub mod log;
pub mod options;
pub mod package;

use std::process::ExitCode;

use xh_reports::prelude::*;

use crate::{log::{Logger, log_report}, options::{OPTIONS, Options, cli::Action, get_opts}};

fn init() -> Result<(), ()> {
    Logger::init();

    OPTIONS
        .set(Options::run()?)
        .ok()
        .expect("options should not be set");

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(report) = init() {
        log_report(&report);
        return ExitCode::FAILURE;
    }

    if let Err(report) = match &get_opts().cli.action {
        Action::Package { project, action } => package::handle(project, action).await.erased(),
        Action::Archive(action) => archive::handle(action).erased(),
    } {
        log_report(&report);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

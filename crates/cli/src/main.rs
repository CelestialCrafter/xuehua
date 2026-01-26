pub mod archive;
pub mod options;
pub mod package;

use std::process::ExitCode;

use smol_str::ToSmolStr;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};
use xh_reports::{
    prelude::*,
    render::{GlobalRenderer, PrettyRenderer},
    tracing::ReportLayer,
};

use crate::options::{OPTIONS, Options, action::Action, get_opts};

fn init() -> Result<(), ()> {
    // TODO: support json rendering via cli arg
    // TODO: add color flag to use with pretty renderer
    GlobalRenderer::set(PrettyRenderer::default());

    // TODO: accept directives via flag instead of environment variable
    let env_layer = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .wrap_with_fn(|| {
            let msg = format_args!(
                "could not parse the {:?} environment variable",
                EnvFilter::DEFAULT_ENV
            );

            Report::new(msg.to_smolstr())
        })?;
    tracing_subscriber::registry()
        .with(ReportLayer::new())
        .with(env_layer)
        .init();

    OPTIONS
        .set(Options::run()?)
        .ok()
        .expect("options should not be set");

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(report) = init() {
        tracing::info!(
            error = &report.into_error() as &dyn StdError,
            "failure initializing cli"
        );

        return ExitCode::FAILURE;
    }

    if let Err(report) = match &get_opts().action {
        Action::Package { project, action } => package::handle(project, action).await.erased(),
        Action::Archive(action) => archive::handle(action).erased(),
    } {
        tracing::error!(
            error = &report.into_error() as &dyn StdError,
            "failure executing action"
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

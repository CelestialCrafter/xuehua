pub mod archive;
pub mod options;
pub mod package;

use tracing::error;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};
use xh_reports::{
    layer::ReportLayer,
    prelude::*,
    render::{GlobalRenderer, PrettyRenderer},
};

use crate::options::{OPTIONS, Options, action::Action, base::BaseOptions, get_opts};

fn init() -> Result<(), ()> {
    // tracing setup
    // TODO: support json rendering via cli arg
    // TODO: add color flag to use with pretty renderer
    GlobalRenderer::set(PrettyRenderer::default());

    // TODO: accept directives via flag instead of environment variable
    let env_layer = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .erased()?;
    tracing_subscriber::registry()
        .with(ReportLayer::new())
        .with(env_layer)
        .init();

    // options setup
    let action = Action::parse_or_exit();
    let base = BaseOptions::read().erased()?;

    let options = Options::apply(base, action);
    OPTIONS.set(options).ok().unwrap();

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(report) = init() {
        error!(
            report = &report.into_error() as &dyn StdError,
            "could not initialize main"
        );

        return;
    }

    if let Err(report) = match &get_opts().action {
        Action::Package { project, action } => package::handle(project, action).await.erased(),
        Action::Archive(action) => archive::handle(action).erased(),
    } {
        error!(
            report = &report.into_error() as &dyn StdError,
            "could not execute command action"
        );

        return;
    }
}

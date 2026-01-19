pub mod archive;
pub mod options;
pub mod package;

use std::io::stderr;

use eyre::{Context, ContextCompat, DefaultHandler, Result};

use log::LevelFilter;
use thiserror::Error;
use xh_reports::{IntoReport, LogError, render::{PrettyRenderer, Render}};

use crate::options::{OPTIONS, Options, cli::Action, get_opts};


#[tokio::main]
async fn main() -> Result<()> {
    // init errors
    eyre::set_hook(Box::new(DefaultHandler::default_with))
        .wrap_err("could not install eyre handler")?;

    // init logging
    let renderer = PrettyRenderer::new();
    fern::Dispatch::new()
        .format(move |out, _, record| {
            let report = LogError::new(record).into_report();
            let display = renderer.render(&report);
            out.finish(format_args!("{display}"))
        })
        .level(LevelFilter::Debug)
        .chain(stderr())
        .apply()
        .wrap_err("could not install logger")?;

    // init opts
    OPTIONS
        .set(Options::run()?)
        .ok()
        .wrap_err("could not resolve options")?;

    // actions
    match &get_opts().cli.action {
        Action::Package { project, action } => package::handle(project, action).await?,
        Action::Archive(action) => archive::handle(action)?,
    }

    Ok(())
}

pub mod archive;
pub mod options;
pub mod package;

use std::io::stderr;

use eyre::{Context, ContextCompat, DefaultHandler, Result};

use fern::colors::{Color, ColoredLevelConfig};
use jiff::Timestamp;
use log::LevelFilter;

use crate::options::{OPTIONS, Options, cli::Action, get_opts};

#[tokio::main]
async fn main() -> Result<()> {
    // init errors
    eyre::set_hook(Box::new(DefaultHandler::default_with))
        .wrap_err("could not install eyre handler")?;

    // init logging
    let colors = ColoredLevelConfig::new()
        .info(Color::Blue)
        .debug(Color::Magenta)
        .trace(Color::BrightBlack)
        .warn(Color::Yellow)
        .error(Color::Red);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "({}) ({}) {} {}",
                // TODO: color timestamp as trace
                Timestamp::now().strftime("%T"),
                colors.color(record.level()).to_string().to_lowercase(),
                record.target(),
                message
            ))
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

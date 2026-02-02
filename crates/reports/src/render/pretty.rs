//! Pretty rendering for [`Report`]s.

use std::fmt;

use log::Level;
use owo_colors::{OwoColorize, Style};

use crate::{Frame, Report, render::Render};

/// Styles for each log level.
#[derive(Debug, Copy, Clone)]
pub struct LogStyles {
    error: Style,
    warn: Style,
    info: Style,
    debug: Style,
    trace: Style,
}

impl Default for LogStyles {
    fn default() -> Self {
        Self {
            error: Style::new().red(),
            warn: Style::new().yellow(),
            info: Style::new().blue(),
            debug: Style::new().magenta(),
            trace: Style::new().white(),
        }
    }
}

/// Styles for each component.
#[derive(Debug, Copy, Clone)]
pub struct Styles {
    guides: Style,
    context: Style,
    suggestion: Style,
    attachment: Style,
    location: Style,
    type_name: Style,
    distracting: Style,
    log: LogStyles,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            guides: Style::new(),
            suggestion: Style::new().green(),
            context: Style::new().cyan(),
            attachment: Style::new().yellow(),
            location: Style::new().purple(),
            type_name: Style::new().blue(),
            distracting: Style::new(),
            log: LogStyles::default(),
        }
    }
}

/// Display characters for each guide.
#[derive(Debug, Copy, Clone)]
pub struct Guides {
    line: &'static str,
    empty: &'static str,
    branch: &'static str,
    last_branch: &'static str,
}

impl Default for Guides {
    fn default() -> Self {
        Self {
            line: "│ ",
            empty: "  ",
            branch: "├─",
            last_branch: "╰─",
        }
    }
}

/// Display characters for each log header.
#[derive(Debug, Copy, Clone)]
pub struct LogHeaders {
    error: &'static str,
    warn: &'static str,
    info: &'static str,
    debug: &'static str,
    trace: &'static str,
}

impl Default for LogHeaders {
    fn default() -> Self {
        Self {
            error: "(error)",
            warn: "(warn)",
            info: "(info)",
            debug: "(debug)",
            trace: "(trace)",
        }
    }
}

/// Display characters for each header.
#[derive(Debug, Copy, Clone)]
pub struct Headers {
    context: &'static str,
    suggestion: &'static str,
    attachment: &'static str,
    type_name: &'static str,
    location: &'static str,
    log: LogHeaders,
}

impl Default for Headers {
    fn default() -> Self {
        Self {
            context: "(context)",
            suggestion: "(suggestion)",
            attachment: "(attachment)",
            type_name: "(type)",
            location: "(location)",
            log: LogHeaders::default(),
        }
    }
}

/// Configuration for [`PrettyRenderer`].
#[derive(Default, Debug, Copy, Clone)]
pub struct Config {
    guides: Guides,
    headers: Headers,
    styles: Styles,
}

/// Pretty renderer for [`Report`]s.
///
/// [`Report`]s can be rendered via the [`Report`] trait.
#[derive(Default, Debug, Copy, Clone)]
pub struct PrettyRenderer {
    /// Configuration for this renderer
    pub config: Config,
}

impl PrettyRenderer {
    /// Constructs a new `PrettyRenderer`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }
}

impl Render for PrettyRenderer {
    fn render<'a, E>(&'a self, report: &'a Report<E>) -> impl fmt::Display + 'a {
        PrettyDisplayer {
            inner: self,
            report,
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct PrettyDisplayer<'a, E> {
    inner: &'a PrettyRenderer,
    report: &'a Report<E>,
}

impl<E> fmt::Display for PrettyDisplayer<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.render_report(f, self.report, format_args!(""), format_args!(""))
    }
}

impl<E> PrettyDisplayer<'_, E> {
    fn render_report<F>(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        report: &Report<F>,
        prefix: fmt::Arguments<'_>,
        next_prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        let headers = &self.inner.config.headers;
        let guides = &self.inner.config.guides;
        let styles = &self.inner.config.styles;

        writeln!(
            fmt,
            "{prefix}{} {}",
            match report.level() {
                Level::Error => headers.log.error.style(styles.log.error),
                Level::Warn => headers.log.warn.style(styles.log.warn),
                Level::Info => headers.log.info.style(styles.log.info),
                Level::Debug => headers.log.debug.style(styles.log.debug),
                Level::Trace => headers.log.trace.style(styles.log.trace),
            },
            report.message().bold()
        )?;

        let children = report.children();
        let guide = if children.is_empty() {
            guides.empty
        } else {
            guides.line
        };

        let sub_prefix = format_args!("{}{}", next_prefix, guide.style(styles.guides));
        self.render_frames(fmt, report.frames(), sub_prefix)?;
        self.render_extra(fmt, report, sub_prefix)?;

        self.render_children(fmt, children, next_prefix)
    }

    fn render_extra<F>(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        report: &Report<F>,
        prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        let headers = &self.inner.config.headers;
        let styles = &self.inner.config.styles;

        writeln!(
            fmt,
            "{prefix}{} {}",
            headers.location.style(styles.location),
            report.location().style(styles.distracting)
        )?;

        writeln!(
            fmt,
            "{prefix}{} {}",
            headers.type_name.style(styles.type_name),
            report.type_name().style(styles.distracting)
        )?;

        Ok(())
    }

    // loops over every frame n times because sorting would require
    // allocation and we aren't going to be handling many frames anyways
    fn render_frames(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        frames: &[Frame],
        prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        let headers = &self.inner.config.headers;
        let styles = &self.inner.config.styles;

        // suggestion pass
        for frame in frames {
            let Frame::Suggestion(suggestion) = frame else {
                continue;
            };

            writeln!(
                fmt,
                "{prefix}{} {}",
                headers.suggestion.style(styles.suggestion),
                suggestion
            )?;
        }

        // context pass
        let mut first = true;
        for frame in frames {
            let Frame::Context((key, value)) = frame else {
                continue;
            };

            if first {
                writeln!(fmt, "{prefix}{}", headers.context.style(styles.context))?;
                first = false;
            }

            writeln!(
                fmt,
                "{prefix}  {}",
                format_args!("{key}: {value}").style(styles.distracting)
            )?;
        }

        // attachment pass
        for frame in frames {
            let Frame::Attachment(attachment) = frame else {
                continue;
            };

            writeln!(
                fmt,
                "{prefix}{}",
                headers.attachment.style(styles.attachment)
            )?;

            for line in attachment.lines() {
                writeln!(fmt, "{prefix}  {}", line.style(styles.distracting))?;
            }
        }

        Ok(())
    }

    fn render_children(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        children: &[Report<()>],
        prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        let guides = &self.inner.config.guides;
        let styles = &self.inner.config.styles;

        let mut children = children.iter().peekable();
        while let Some(child) = children.next() {
            let last = children.peek().is_none();

            let guide = if last {
                guides.last_branch
            } else {
                guides.branch
            };
            let next_prefix = format_args!("{}{}", prefix, guide.style(styles.guides));

            let guide = if last { guides.empty } else { guides.line };
            let next_next_prefix = format_args!("{}{}", prefix, guide.style(styles.guides));

            self.render_report(fmt, &child, next_prefix, next_next_prefix)?;
        }

        Ok(())
    }
}

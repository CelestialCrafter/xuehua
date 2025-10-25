use alloc::vec::Vec;
use core::fmt;

use owo_colors::{OwoColorize, Style};

use crate::{Frame, Report, ReportInner, render::Renderer};

pub struct Styles {
    guides: Style,
    error: Style,
    context: Style,
    suggestion: Style,
    attachment: Style,
    location: Style,
    distracting: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            guides: Style::new(),
            error: Style::new().red(),
            context: Style::new().cyan(),
            suggestion: Style::new().green(),
            attachment: Style::new().blue(),
            location: Style::new().purple(),
            distracting: Style::new(),
        }
    }
}

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

pub struct Headers {
    error: &'static str,
    context: &'static str,
    suggestion: &'static str,
    attachment: &'static str,
    location: &'static str,
}

impl Default for Headers {
    fn default() -> Self {
        Self {
            error: "(error)",
            context: "(context)",
            suggestion: "(suggestion)",
            attachment: "(attachment)",
            location: "(location)",
        }
    }
}

#[derive(Default)]
pub struct Config {
    guides: Guides,
    headers: Headers,
    styles: Styles,
}

#[derive(Default)]
pub struct HumanRenderer {
    pub config: Config,
}

impl Renderer for HumanRenderer {
    fn render<E>(&self, fmt: &mut fmt::Formatter<'_>, report: &Report<E>) -> fmt::Result {
        self.render_report(fmt, &report.inner, format_args!(""), format_args!(""))
    }
}

impl HumanRenderer {
    fn render_report(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        report: &ReportInner,
        prefix: fmt::Arguments<'_>,
        next_prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        writeln!(
            fmt,
            "{prefix}{} {}",
            self.config.headers.error.style(self.config.styles.error),
            report.error.bold()
        )?;

        self.render_frames(
            fmt,
            report.frames.as_slice(),
            format_args!(
                "{}{}",
                next_prefix,
                if report.children.is_empty() {
                    self.config.guides.empty
                } else {
                    self.config.guides.line
                }
                .style(self.config.styles.guides)
            ),
        )?;

        self.render_children(fmt, &report.children, next_prefix)
    }

    fn render_frames(
        &self,
        fmt: &mut fmt::Formatter<'_>,
        frames: &[Frame],
        prefix: fmt::Arguments<'_>,
    ) -> fmt::Result {
        let mut sorted: Vec<_> = frames.iter().collect();
        sorted.sort_by_key(|f| match f {
            Frame::Suggestion(_) => 0,
            Frame::Location(_) => 1,
            Frame::Context(_) => 2,
            Frame::Attachment(_) => 3,
        });

        for frame in sorted {
            match frame {
                Frame::Context(context) => {
                    writeln!(
                        fmt,
                        "{prefix}{}",
                        self.config
                            .headers
                            .context
                            .style(self.config.styles.context)
                    )?;

                    for (k, v) in context {
                        writeln!(
                            fmt,
                            "{prefix}  {}",
                            format_args!("{k}: {v}").style(self.config.styles.distracting)
                        )?;
                    }
                }
                Frame::Suggestion(suggestion) => {
                    writeln!(
                        fmt,
                        "{prefix}{} {}",
                        self.config
                            .headers
                            .suggestion
                            .style(self.config.styles.suggestion),
                        suggestion
                    )?;
                }
                Frame::Attachment(attachment) => {
                    writeln!(
                        fmt,
                        "{prefix}{}",
                        self.config
                            .headers
                            .attachment
                            .style(self.config.styles.attachment)
                    )?;

                    for line in attachment.lines() {
                        writeln!(
                            fmt,
                            "{prefix}  {}",
                            line.style(self.config.styles.distracting)
                        )?;
                    }
                }
                Frame::Location(location) => {
                    writeln!(
                        fmt,
                        "{prefix}{} {}",
                        self.config
                            .headers
                            .location
                            .style(self.config.styles.location),
                        location.style(self.config.styles.distracting)
                    )?;
                }
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
        let mut children = children.iter().peekable();
        while let Some(child) = children.next() {
            let last = children.peek().is_none();

            let guide = if last {
                self.config.guides.last_branch
            } else {
                self.config.guides.branch
            };
            let next_prefix = format_args!("{}{}", prefix, guide.style(self.config.styles.guides));

            let guide = if last {
                self.config.guides.empty
            } else {
                self.config.guides.line
            };
            let next_next_prefix =
                format_args!("{}{}", prefix, guide.style(self.config.styles.guides));

            self.render_report(fmt, &child.inner, next_prefix, next_next_prefix)?;
        }

        Ok(())
    }
}

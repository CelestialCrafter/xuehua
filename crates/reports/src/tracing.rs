/// Utilities for interoperating with the [`tracing`] crate.
use std::{any::type_name, error::Error, fmt, io::Write};

use smol_str::{SmolStr, ToSmolStr};
use tracing::{Event, Subscriber, field, level_filters::LevelFilter, span};
use tracing_subscriber::{Layer, fmt::MakeWriter, layer::Context, registry::LookupSpan};

use crate::{
    Frame, Level, Location, Metadata, Report, ReportError, ReportPayload,
    render::{GlobalRenderer, Renderer},
};

#[derive(Default)]
struct RecordVisitor {
    message: Option<SmolStr>,
    frames: Vec<Frame>,
    children: Vec<ReportPayload>,
}

impl field::Visit for RecordVisitor {
    fn record_str(&mut self, field: &field::Field, value: &str) {
        match field.name() {
            "suggestion" => self.frames.push(Frame::suggestion(value)),
            _ => self.record_debug(field, &value),
        }
    }

    fn record_debug(&mut self, field: &field::Field, value: &dyn fmt::Debug) {
        if field.name().starts_with("log.") {
            return;
        }

        let value = format_args!("{value:?}");
        match field.name() {
            "message" => self.message = Some(value.to_smolstr()),
            "attachment" => self.frames.push(Frame::attachment(value)),
            key => self.frames.push(Frame::context(key, value)),
        }
    }

    fn record_error(&mut self, _field: &field::Field, value: &(dyn Error + 'static)) {
        self.children
            .push(match value.downcast_ref::<ReportError>() {
                Some(error) => error.0.clone(),
                None => Report::from_error(value).into_payload(),
            });
    }
}

fn metadata_to_payload(metadata: &tracing::Metadata, visitor: RecordVisitor) -> ReportPayload {
    let message = if metadata.is_span() {
        metadata.name().to_smolstr()
    } else {
        visitor
            .message
            .unwrap_or(SmolStr::new_static("no message specified"))
    };

    let level = match *metadata.level() {
        tracing::Level::TRACE => Level::Trace,
        tracing::Level::DEBUG => Level::Debug,
        tracing::Level::INFO => Level::Info,
        tracing::Level::WARN => Level::Warn,
        tracing::Level::ERROR => Level::Error,
    };

    let type_name = SmolStr::new_static(type_name::<Event>());
    let location = metadata
        .file()
        .map(|name| Location::File {
            name: name.into(),
            line: metadata.line(),
            column: None,
        })
        .unwrap_or_else(|| Location::Module(metadata.target().into()));

    ReportPayload {
        frames: visitor.frames,
        children: visitor.children,
        message,
        metadata: Metadata {
            location,
            level,
            type_name,
        },
    }
}

/// A [`Layer`] that logs [`Event`]s to a report [`Renderer`].
pub struct ReportLayer<R, W> {
    renderer: R,
    writer: W,
    render_spans_at: LevelFilter,
}

impl ReportLayer<GlobalRenderer, fn() -> std::io::Stderr> {
    /// Constructs a new `ReportLayer`.
    pub fn new() -> Self {
        Self {
            renderer: GlobalRenderer,
            writer: std::io::stderr,
            render_spans_at: LevelFilter::DEBUG,
        }
    }
}

impl<R, W> ReportLayer<R, W> {
    /// Sets the [`Renderer`] this layer uses to display reports.
    pub fn with_renderer<T>(self, renderer: T) -> ReportLayer<T, W> {
        ReportLayer {
            renderer,
            writer: self.writer,
            render_spans_at: self.render_spans_at,
        }
    }

    /// Sets the [`MakeWriter`] this layer uses to write events.
    pub fn with_writer<X>(self, writer: X) -> ReportLayer<R, X> {
        ReportLayer {
            renderer: self.renderer,
            writer,
            render_spans_at: self.render_spans_at,
        }
    }

    /// Control whether spans are rendered as parents of events or just hidden.
    pub fn render_spans(mut self, at: LevelFilter) -> Self {
        self.render_spans_at = at;
        self
    }
}

impl<S, R, W> Layer<S> for ReportLayer<R, W>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    R: Renderer + 'static,
    W: for<'a> MakeWriter<'a> + 'static,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let mut visitor = RecordVisitor::default();
        attrs.record(&mut visitor);
        let payload = metadata_to_payload(span.metadata(), visitor);
        span.extensions_mut().insert(payload);
    }

    fn on_record(&self, span: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(span) else {
            return;
        };

        let mut extensions = span.extensions_mut();
        let Some(payload) = extensions.get_mut::<ReportPayload>() else {
            return;
        };

        let mut visitor = RecordVisitor::default();
        values.record(&mut visitor);

        payload.frames.extend(visitor.frames);
        payload.children.extend(visitor.children);
        if let Some(message) = visitor.message {
            payload.message = message;
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = RecordVisitor::default();
        event.record(&mut visitor);

        let metadata = event.metadata();
        let mut payload = metadata_to_payload(metadata, visitor);
        if let Some(scope) = ctx.event_scope(event) {
            let render_spans = *metadata.level() <= self.render_spans_at;
            payload = if render_spans {
                scope.fold(payload, |acc, span| {
                    match span.extensions().get::<ReportPayload>() {
                        Some(parent) => {
                            let mut parent = parent.clone();
                            parent.children.push(acc);
                            parent
                        }
                        None => acc,
                    }
                })
            } else {
                let frames = scope
                    .filter_map(|span| {
                        span.extensions()
                            .get::<ReportPayload>()
                            .map(|p| p.frames.clone().into_iter())
                    })
                    .flatten();

                payload.frames.extend(frames);
                payload
            }
        }

        let display = self.renderer.render(&payload);
        let mut writer = self.writer.make_writer_for(metadata);
        let _ = writeln!(writer, "{}", display);
    }
}

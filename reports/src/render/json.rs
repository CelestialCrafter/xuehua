use core::fmt;

use serde_json::{Map, Value, json};

use crate::{Frame, Report, render::Renderer};

#[derive(Default)]
pub struct JsonRenderer {
    pub pretty: bool,
}

impl Renderer for JsonRenderer {
    fn render<E>(&self, fmt: &mut fmt::Formatter<'_>, report: &Report<E>) -> fmt::Result {
        let value = Self::report_to_value(report);
        if self.pretty {
            writeln!(fmt, "{:#}", value)
        } else {
            writeln!(fmt, "{}", value)
        }
    }
}

impl JsonRenderer {
    fn report_to_value<E>(report: &Report<E>) -> Value {
        let frames: Vec<_> = report
            .inner
            .frames
            .iter()
            .map(|frame| {
                json!({
                    match frame {
                        Frame::Location(_) => "location",
                        Frame::Context(_) => "context",
                        Frame::Attachment(_) => "attachment",
                        Frame::Suggestion(_) => "suggestion"
                    }: Self::frame_to_value(frame)
                })
            })
            .collect();

        let children: Vec<_> = report
            .inner
            .children
            .iter()
            .map(Self::report_to_value)
            .collect();

        json!({
            "error": report.inner.error.to_string(),
            "frames": frames,
            "children": children
        })
    }

    fn frame_to_value(frame: &Frame) -> Value {
        match frame {
            Frame::Location(location) => location.to_string().into(),
            Frame::Context(context) => context
                .iter()
                .fold(Map::new(), |mut acc, (key, value)| {
                    acc.insert(key.to_string(), value.clone().into());
                    acc
                })
                .into(),
            Frame::Attachment(attachment) => attachment.clone().into(),
            Frame::Suggestion(suggestion) => suggestion.clone().into(),
        }
    }
}

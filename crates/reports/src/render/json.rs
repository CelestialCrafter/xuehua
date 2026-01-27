//! [JSON](https://json.org/) rendering for [`Report`]s.

use alloc::{string::ToString, vec::Vec};
use core::fmt;

use serde_json::{Value, json};

use crate::{Frame, Report, render::Render};

#[derive(Debug, Clone)]
struct JsonDisplayer<'a> {
    inner: &'a JsonRenderer,
    value: Value,
}

impl fmt::Display for JsonDisplayer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.pretty {
            writeln!(f, "{:#}", self.value)
        } else {
            writeln!(f, "{}", self.value)
        }
    }
}

/// [JSON](https://json.org/) renderer for [`Report`]s.
///
/// [`Report`]s can be rendered via the [`Report`] trait.
#[derive(Default, Debug, Copy, Clone)]
pub struct JsonRenderer {
    /// Whether or not the output is pretty-printed.
    pub pretty: bool,
}

impl JsonRenderer {
    /// Constructs a new `JsonRenderer`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }
}

impl Render for JsonRenderer {
    fn render<E>(&self, report: &Report<E>) -> impl fmt::Display {
        JsonDisplayer {
            inner: self,
            value: report_to_value(report),
        }
    }
}

fn report_to_value<E>(report: &Report<E>) -> Value {
    let frames: Vec<_> = report
        .inner
        .frames
        .iter()
        .map(|frame| {
            json!({
                "type": match frame {
                    Frame::Context(_) => "context",
                    Frame::Attachment(_) => "attachment",
                    Frame::Suggestion(_) => "suggestion"
                },
                "value": frame_to_value(frame)
            })
        })
        .collect();

    let children: Vec<_> = report.inner.children.iter().map(report_to_value).collect();

    json!({
        "error": report.to_string(),
        "location": report.location().to_string(),
        "level": report.level().to_string(),
        "type": report.type_name(),
        "frames": frames,
        "children": children
    })
}

fn frame_to_value(frame: &Frame) -> Value {
    match frame {
        Frame::Context(context) => json!({
            "key": *context.0,
            "value": context.1
        }),
        Frame::Attachment(attachment) => attachment.clone().into(),
        Frame::Suggestion(suggestion) => suggestion.to_string().into(),
    }
}

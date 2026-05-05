//! [JSON](https://json.org/) rendering for [`Report`]s.

use std::fmt;

use crate::{ReportPayload, render::Renderer};

#[derive(Debug, Clone)]
struct JsonDisplayer<'a> {
    inner: &'a JsonRenderer,
    payload: &'a ReportPayload,
}

impl fmt::Display for JsonDisplayer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let serialize = if self.inner.pretty {
            serde_json::to_string_pretty
        } else {
            serde_json::to_string
        };

        let string = serialize(self.payload).expect("report serialization should succeed");
        f.write_str(&string)
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

impl Renderer for JsonRenderer {
    fn render<'a>(&'a self, payload: &'a ReportPayload) -> impl fmt::Display + 'a {
        JsonDisplayer {
            inner: self,
            payload,
        }
    }
}

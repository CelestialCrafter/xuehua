//! Default rendering for [`Report`]s.

use std::fmt;

use crate::{ReportPayload, render::Renderer};

/// Default renderer for [`Report`]s.
///
/// [`Report`]s can be rendered via the [`Report`] trait.
#[derive(Debug, Copy, Clone, Default)]
pub struct SimpleRenderer;

impl SimpleRenderer {
    /// Constructs a new `SimpleRenderer`.
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Renderer for SimpleRenderer {
    fn render(&self, report: &ReportPayload) -> impl fmt::Display {
        &report.message
    }
}

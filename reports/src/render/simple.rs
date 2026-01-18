//! Default rendering for [`Report`]s.

use core::fmt;

use crate::{Report, render::Render};

/// Default renderer for [`Report`]s.
///
/// [`Report`]s can be rendered via the [`Report`] trait.
pub struct SimpleRenderer;

impl Render for SimpleRenderer {
    fn render<E>(&self, report: &Report<E>) -> impl fmt::Display {
        report.error()
    }
}

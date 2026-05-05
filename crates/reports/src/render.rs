//! Rendering for [`Report`]s in various formats.

pub mod global;
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "pretty")]
pub mod pretty;
pub mod simple;

use std::fmt;

pub use global::GlobalRenderer;
#[cfg(feature = "json")]
pub use json::JsonRenderer;
#[cfg(feature = "pretty")]
pub use pretty::PrettyRenderer;
pub use simple::SimpleRenderer;

use crate::ReportPayload;

/// Trait for rendering [`Report`]s.
pub trait Renderer {
    /// Renders a [`Report`] into an impl [`fmt::Display`]
    fn render<'a>(&'a self, payload: &'a ReportPayload) -> impl fmt::Display + 'a;
}

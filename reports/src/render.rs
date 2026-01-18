//! Rendering for [`Report`]s in various formats.

#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "pretty")]
pub mod pretty;
pub mod simple;

use core::fmt;

#[cfg(feature = "json")]
pub use json::JsonRenderer;
#[cfg(feature = "pretty")]
pub use pretty::PrettyRenderer;
pub use simple::SimpleRenderer;

use crate::Report;

/// Trait for rendering [`Report`]s.
pub trait Render {
    /// Renders a [`Report`] into an impl [`fmt::Display`]
    fn render<'a, E>(&'a self, report: &'a Report<E>) -> impl fmt::Display + 'a;
}

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

/// Sets a panic hook for rendering [`Report`]s.
#[cfg(feature = "std")]
pub fn set_hook(renderer: impl Render + Send + Sync + 'static) {
    std::panic::set_hook(std::boxed::Box::new(move |info| {
        let message = info.payload_as_str().unwrap_or("no message");
        renderer.render(&Report::<&str>::new(message));
    }));
}

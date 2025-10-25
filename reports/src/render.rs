#[cfg(feature = "json")]
pub mod json;
pub mod human;

#[cfg(feature = "json")]
pub use json::JsonRenderer;
pub use human::HumanRenderer;

use core::fmt;

use crate::Report;

pub trait Renderer {
    fn render<E>(&self, fmt: &mut fmt::Formatter<'_>, report: &Report<E>) -> fmt::Result;
}

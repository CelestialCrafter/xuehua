#[cfg(feature = "pretty")]
pub mod pretty;
pub mod simple;

use core::fmt;

#[cfg(feature = "pretty")]
pub use pretty::PrettyRenderer;
pub use simple::SimpleRenderer;

use crate::Report;

pub trait Render {
    fn render<'a, E>(&'a self, report: &'a Report<E>) -> impl fmt::Display + 'a;
}

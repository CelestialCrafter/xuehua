pub mod simple;

use core::fmt;

pub use simple::SimpleRenderer;

use crate::Report;

pub trait Render {
    fn render<'a, E>(&'a self, report: &'a Report<E>) -> impl fmt::Display + 'a;
}

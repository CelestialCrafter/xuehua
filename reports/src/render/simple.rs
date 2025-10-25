use core::fmt;

use crate::{Report, render::Render};

pub struct SimpleRenderer;

impl Render for SimpleRenderer {
    fn render<E>(&self, report: &Report<E>) -> impl fmt::Display {
        report.error()
    }
}

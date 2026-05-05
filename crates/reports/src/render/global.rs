//! Rendering proxy to a global report [`Renderer`].

use std::{
    fmt::{self, Display},
    sync::OnceLock,
};

use crate::{
    ReportPayload,
    render::{Renderer, SimpleRenderer},
};

static RENDERER: OnceLock<Box<dyn DynRender + Send + Sync>> = OnceLock::new();

trait DynRender {
    fn render(&self, payload: &ReportPayload, fmt: &mut fmt::Formatter) -> fmt::Result;
}

impl<T: Renderer> DynRender for T {
    fn render(&self, payload: &ReportPayload, f: &mut fmt::Formatter) -> fmt::Result {
        Renderer::render(self, payload).fmt(f)
    }
}

struct GlobalDisplayer<'a> {
    payload: &'a ReportPayload,
}

impl fmt::Display for GlobalDisplayer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let renderer = RENDERER.get().map(Box::as_ref).unwrap_or(&SimpleRenderer);
        renderer.render(&self.payload, f)
    }
}

/// Proxy renderer to the global [`Renderer`], set by [`GlobalRenderer::set`].
/// If there is no global renderer registered, the impl falls back to using a [`SimpleRenderer`].
#[derive(Debug, Copy, Clone, Default)]
pub struct GlobalRenderer;

impl GlobalRenderer {
    /// Sets the global [`Renderer`].
    /// The global renderer can only be set once and applies to the entire program,
    /// so libraries should avoid calling this function.
    ///
    /// Returns `None` if a renderer was already set.
    pub fn set(renderer: impl Renderer + Send + Sync + 'static) -> Option<()> {
        RENDERER.set(Box::new(renderer)).ok()
    }
}

impl Renderer for GlobalRenderer {
    fn render(&self, payload: &ReportPayload) -> impl fmt::Display {
        GlobalDisplayer { payload }
    }
}

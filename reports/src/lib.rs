//! Error handling crate designed for use with Xuehua

#![warn(missing_docs)]
#![no_std]

extern crate alloc;

pub use xh_reports_derive::IntoReport;
pub mod render;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{any::type_name, error::Error, fmt, marker::PhantomData, panic::Location};

use educe::Educe;
use log::{
    Level,
    kv::{Key, Value, VisitSource},
};
use smol_str::{SmolStr, ToSmolStr};
use thiserror::Error;

use crate::render::{Render, SimpleRenderer};

/// Utility alias for [`Error`]
pub type BoxDynError = Box<dyn Error + Send + Sync + 'static>;

/// A single piece of information inside of a [`Report`].
///
/// `Frame`s can be created via the [`Self::suggestion`], [`Self::context`], or [`Self::attachment`] methods.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    /// A collection of keys and values.
    ///
    /// This can be used to attach additional data such as:
    /// ids, timestamps, commands, etc.
    Context(Vec<(SmolStr, String)>),
    /// A long-form piece of information.
    ///
    /// This can be used to attach more detailed data such as:
    /// stderr, build logs, files, etc.
    Attachment(String),
    /// An inline suggestion
    ///
    /// This can be used to suggest actions to users to resolve issues.
    Suggestion(SmolStr),
}

impl Frame {
    /// Helper function to create [`Self::Context`]s.
    pub fn context<K, V, I>(context: I) -> Self
    where
        K: Into<SmolStr>,
        V: fmt::Display,
        I: IntoIterator<Item = (K, V)>,
    {
        let context = context
            .into_iter()
            .map(|(key, value): (K, V)| (key.into(), value.to_string()))
            .collect();
        Self::Context(context)
    }

    /// Helper function to create [`Self::Suggestion`]s.
    pub fn suggestion(suggestion: impl Into<SmolStr>) -> Frame {
        Self::Suggestion(suggestion.into())
    }

    /// Helper function to create [`Self::Attachment`]s.
    pub fn attachment(attachment: impl fmt::Display) -> Frame {
        Self::Attachment(attachment.to_string())
    }
}

/// Typestate for an erased report.
#[derive(Error, Debug)]
#[error("")]
pub struct Erased {
    private: (),
}

#[derive(Debug)]
struct ReportInner {
    frames: Vec<Frame>,
    children: Vec<Report<Erased>>,
    error: BoxDynError,
    type_name: &'static str,
    location: &'static Location<'static>,
    level: Level,
}

/// A tree of errors.
///
/// Each report contains [`Frame`]s, child [`Report`]s,
/// and additional information about the error.
#[derive(Educe)]
#[educe(Debug(bound()))]
pub struct Report<E> {
    inner: Box<ReportInner>,
    _marker: PhantomData<E>,
}

#[derive(Error, Debug)]
#[error("{0}")]
struct SourceError(String);

impl<E> Report<E> {
    /// Constructs a new [`Report`] from an error.
    #[track_caller]
    pub fn new(error: E) -> Self
    where
        E: Error + 'static,
        E: Send + Sync,
    {
        fn walk(error: &dyn Error, location: &'static Location<'static>) -> Vec<Report<Erased>> {
            if let Some(source) = error.source() {
                vec![Report {
                    inner: Box::new(ReportInner {
                        children: walk(source, location),
                        error: Box::new(SourceError(source.to_string())),
                        frames: Default::default(),
                        type_name: type_name::<SourceError>(),
                        location,
                        level: Level::Error,
                    }),
                    _marker: PhantomData,
                }]
            } else {
                vec![]
            }
        }

        let location = Location::caller();
        Self {
            inner: Box::new(ReportInner {
                children: walk(&error, location),
                error: Box::new(error),
                frames: Default::default(),
                type_name: type_name::<E>(),
                location,
                level: Level::Error,
            }),
            _marker: PhantomData,
        }
    }

    /// Retrieves the error associated with this `Report`.
    pub fn error(&self) -> &(dyn Error + Send + Sync) {
        &*self.inner.error
    }

    /// Retrieves the type name that the `Report` was created with.
    ///
    /// This method has the same semantics as [`core::any::type_name`].
    pub fn type_name(&self) -> &'static str {
        self.inner.type_name
    }

    /// Retrieves the location this `Report` was created at.
    pub fn location(&self) -> &'static Location<'static> {
        self.inner.location
    }

    /// Erases the `Report`s type to `Erased`.
    pub fn erased(self) -> Report<Erased> {
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }

    /// "Wraps" this `Report` with a parent `Report`.
    pub fn wrap<F: IntoReport>(self, parent: F) -> Report<F> {
        parent.into_report().with_child(self)
    }

    /// Sets the log level associated with the `Report`.
    pub fn with_level(mut self, level: Level) -> Self {
        self.inner.level = level;
        self
    }

    /// Retrieves the log level associated with this `Report`.
    pub fn level(&self) -> Level {
        self.inner.level
    }

    /// Retrieves the [`Frame`]s associated with this `Report`..
    pub fn frames(&self) -> &[Frame] {
        &self.inner.frames
    }

    /// Appends a [`Frame`] to this `Report`.
    pub fn with_frame(mut self, frame: Frame) -> Self {
        self.inner.frames.push(frame);
        self
    }

    /// Appends an iterator of [`Frame`]s to this `Report`.
    pub fn with_frames(self, frames: impl IntoIterator<Item = Frame>) -> Self {
        frames.into_iter().fold(self, |acc, x| acc.with_frame(x))
    }

    /// Retrieves the children associated with this `Report`.
    pub fn children(&self) -> &[Report<Erased>] {
        &self.inner.children
    }

    /// Appends a `Report` as a child to this `Report`.
    pub fn with_child<F>(mut self, child: Report<F>) -> Self {
        self.inner.children.push(child.erased());
        self
    }

    /// Appends an iterator of `Report`s as children of this `Report`.
    pub fn with_children<F>(self, children: impl IntoIterator<Item = Report<F>>) -> Self {
        children.into_iter().fold(self, |acc, x| acc.with_child(x))
    }
}

impl<E> fmt::Display for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        SimpleRenderer.render(self).fmt(f)
    }
}

#[cfg(feature = "auto-erase")]
impl<E> From<E> for Report<Erased>
where
    E: Error + 'static,
    E: Send + Sync,
{
    fn from(value: E) -> Self {
        Report::new(value).erased()
    }
}

/// Helper trait for [`Result<T, Report<E>>`].
pub trait ResultReportExt<T, F, E: Into<Report<F>>> {
    /// Erases the inner [`Report`]s type to [`Erased`].
    ///
    /// See [`Report::erased`] for more information.
    fn erased(self) -> Result<T, Report<Erased>>;

    /// Append a `Report` as a parent of the inner `Report`.
    ///
    /// See [`Report::wrap`] for more information.
    fn wrap_fn<G: IntoReport>(self, parent: impl FnOnce() -> G) -> Result<T, Report<G>>;

    /// Append a `Report` as a parent of the inner `Report`.
    ///
    /// See [`Report::wrap`] for more information.
    fn wrap<G: IntoReport>(self, parent: G) -> Result<T, Report<G>>
    where
        Self: Sized,
    {
        self.wrap_fn(|| parent)
    }

    /// Sets the log level associated with the inner [`Report`].
    ///
    /// See [`Report::with_level`] for more information.
    fn with_level(self, level: Level) -> Result<T, Report<F>>;

    /// Appends a [`Frame`] to the inner [`Report`].
    ///
    /// See [`Report::with_frame`] for more information.
    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, Report<F>>;
}

impl<T, F, E: Into<Report<F>>> ResultReportExt<T, F, E> for Result<T, E> {
    fn erased(self) -> Result<T, Report<Erased>> {
        self.map_err(|report| report.into().erased())
    }

    fn wrap_fn<G: IntoReport>(self, parent: impl FnOnce() -> G) -> Result<T, Report<G>> {
        self.map_err(|report| parent().into_report().with_child(report.into()))
    }

    fn with_level(self, level: Level) -> Result<T, Report<F>> {
        self.map_err(|report| report.into().with_level(level))
    }

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, Report<F>> {
        self.map_err(|report| report.into().with_frame(frame()))
    }
}

/// Trait for converting [`Error`]s into enriched [`Report`]s
///
/// This can be implemented via `#[derive(IntoReport)]`.
/// See [`xh_reports_derive::IntoReport`] for more information.
pub trait IntoReport: Sized + Error + Send + Sync + 'static {
    /// Converts this Error into a [`Report`].
    ///
    /// Consumers should prefer `into_report` over [`Report::new`]
    /// to generate pre-enriched reports.
    ///
    /// Implementations should modify the [`Report`]
    /// (by adding frames, children, etc) as they see fit.
    #[track_caller]
    fn into_report(self) -> Report<Self> {
        Report::new(self)
    }
}

impl<E: IntoReport> From<E> for Report<E> {
    fn from(value: E) -> Self {
        value.into_report()
    }
}
/// Helper struct for converting [`log::Record`]s into [`Report`]s.
///
/// This error can be converted into a [`Report`] via the [`IntoReport`] trait.
#[derive(Error, Debug)]
#[error("{message}")]
pub struct LogError {
    message: String,
    level: Level,
    children: Vec<Report<Erased>>,
    frames: Vec<Frame>,
}

#[derive(Error, Debug)]
#[error("{0}")]
struct LogSubError(String);

impl LogError {
    /// Constructs a new [`LogError`]
    pub fn new(record: &log::Record) -> Self {
        #[derive(Default)]
        struct FrameVisitor {
            frames: Vec<Frame>,
            children: Vec<Report<Erased>>,
            context: Vec<(SmolStr, String)>,
        }

        impl VisitSource<'_> for FrameVisitor {
            fn visit_pair(&mut self, key: Key<'_>, value: Value<'_>) -> Result<(), log::kv::Error> {
                match key.as_str() {
                    "suggestion" => self.frames.push(Frame::Suggestion(value.to_smolstr())),
                    "attachment" => self.frames.push(Frame::Attachment(value.to_string())),
                    "error" => {
                        let report = Report::new(LogSubError(value.to_string())).erased();
                        self.children.push(report)
                    }
                    key => self.context.push((key.into(), value.to_string())),
                }

                Ok(())
            }
        }

        let mut visitor = FrameVisitor::default();
        record.key_values().visit(&mut visitor).unwrap();

        visitor
            .context
            .push(("target".into(), record.target().to_string()));
        visitor.frames.push(Frame::context(visitor.context));

        Self {
            message: record.args().to_string(),
            level: record.level(),
            frames: visitor.frames,
            children: visitor.children,
        }
    }
}

impl IntoReport for LogError {
    fn into_report(mut self) -> Report<Self> {
        let frames = core::mem::take(&mut self.frames);
        let level = self.level;
        let children = core::mem::take(&mut self.children);

        Report::new(self)
            .with_frames(frames)
            .with_level(level)
            .with_children(children)
    }
}

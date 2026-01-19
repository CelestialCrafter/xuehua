//! Error handling crate designed for use with Xuehua

#![warn(missing_docs)]
#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod compat;
pub mod prelude;
pub mod render;

pub use xh_reports_derive::IntoReport;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    any::{type_name, type_name_of_val},
    error::Error,
    fmt,
    marker::PhantomData,
    panic::Location,
    result::Result as CoreResult,
};

use educe::Educe;
use log::{
    Level,
    kv::{Key, Value, VisitSource},
};
use smol_str::{SmolStr, ToSmolStr};

use crate::render::{Render, SimpleRenderer};

/// Helper alias for [`Error`]
pub type BoxDynError = Box<dyn Error + Send + Sync + 'static>;

/// Helper alias for [`Result`](CoreResult)
pub type Result<T, E> = CoreResult<T, Report<E>>;

/// A single piece of information inside of a [`Report`].
///
/// `Frame`s can be created via the [`Self::suggestion`], [`Self::context`], or [`Self::attachment`] methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A collection of keys and values.
    ///
    /// This can be used to attach additional data such as:
    /// ids, timestamps, commands, etc.
    Context((SmolStr, String)),
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
    pub fn context<K, V>(key: K, value: V) -> Self
    where
        K: Into<SmolStr>,
        V: fmt::Display,
    {
        Self::Context((key.into(), value.to_string()))
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

#[derive(Clone, Debug)]
struct ReportInner {
    frames: Vec<Frame>,
    children: Vec<Report<()>>,
    message: SmolStr,
    type_name: &'static str,
    // TODO: replace Location with custom location struct to
    // support providing panic/log location in reports
    location: &'static Location<'static>,
    level: Level,
}

impl ReportInner {
    fn new(
        message: SmolStr,
        type_name: &'static str,
        location: &'static Location<'static>,
    ) -> Self {
        Self {
            message,
            type_name,
            location,
            children: Default::default(),
            frames: Default::default(),
            level: Level::Error,
        }
    }
}

/// A tree of errors.
///
/// Each report contains [`Frame`]s, child [`Report`]s,
/// and additional information about the error.
#[derive(Educe, Clone)]
#[educe(Debug(bound()))]
pub struct Report<E> {
    inner: Box<ReportInner>,
    _marker: PhantomData<E>,
}

impl<T> Report<T> {
    /// Constructs a new [`Report`] from a message.
    #[track_caller]
    pub fn new(message: impl Into<SmolStr>) -> Self {
        Report {
            inner: ReportInner::new(message.into(), type_name::<T>(), Location::caller()).into(),
            _marker: PhantomData,
        }
    }

    /// Constructs a new [`Report`] from an error.
    ///
    /// This method populates children by walking [`Error::source`].
    /// To avoid this, consider using [`Report::new`].
    #[track_caller]
    pub fn from_error(error: impl Error) -> Self {
        fn walk<T>(
            error: &dyn Error,
            type_name: &'static str,
            location: &'static Location<'static>,
        ) -> Report<T> {
            let report = Report {
                inner: ReportInner::new(error.to_smolstr(), type_name, location).into(),
                _marker: PhantomData,
            };

            match error.source() {
                Some(source) => {
                    report.with_child(walk::<()>(source, type_name_of_val(source), location))
                }
                None => report,
            }
        }

        walk(&error, type_name_of_val(&error), Location::caller())
    }

    /// Retrieves the type name that the `Report` was created with.
    ///
    /// # Notes
    ///
    /// This method has the same semantics as [`core::any::type_name`].
    pub fn type_name(&self) -> &'static str {
        self.inner.type_name
    }

    /// Retrieves the location this `Report` was created at.
    pub fn location(&self) -> &'static Location<'static> {
        self.inner.location
    }

    /// Retrieves the message associated with this `Report`.
    pub fn message(&self) -> SmolStr {
        self.inner.message.clone()
    }

    /// "Erases" this `Report`s generic parameter to [`()`].
    pub fn erased(self) -> Report<()> {
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }

    /// Converts this `Report` into a type implementing [`Error`].
    pub fn into_error(self) -> impl Error + Send + Sync + 'static {
        #[derive(Debug)]
        struct ReportError {
            message: SmolStr,
            child: Option<Box<ReportError>>,
        }

        impl ReportError {
            fn new(report: Report<()>) -> Self {
                Self {
                    message: report.inner.message,
                    child: {
                        let mut children = report.inner.children;
                        (children.len() == 1).then(|| Self::new(children.swap_remove(0)).into())
                    },
                }
            }
        }

        impl fmt::Display for ReportError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.message)
            }
        }

        impl Error for ReportError {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                self.child.as_ref().map(|error| error as _)
            }
        }

        ReportError::new(self.erased())
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
    pub fn children(&self) -> &[Report<()>] {
        &self.inner.children
    }

    /// Appends a `Report` as a child to this `Report`.
    pub fn with_child<F>(mut self, child: Report<F>) -> Self {
        self.inner.children.push(Report {
            inner: child.inner,
            _marker: PhantomData,
        });
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

/// Helper trait for [`Report<T>`].
pub trait ReportExt<T> {
    /// "Wraps" this `Report` with the parent's default `Report`.
    ///
    /// See [`Report::wrap`] for more information.
    fn wrap<U: IntoReport + Default>(self) -> Report<U>;

    /// "Wraps" this `Report` with a parent `Report`.
    ///
    /// See [`Report::wrap_with`] for more information.
    fn wrap_with<U>(self, parent: impl Into<Report<U>>) -> Report<U>;
}

impl<T, U: Into<Report<T>>> ReportExt<T> for U {
    #[track_caller]
    fn wrap<V: IntoReport + Default>(self) -> Report<V> {
        V::default().into_report().with_child(self.into())
    }

    fn wrap_with<V>(self, parent: impl Into<Report<V>>) -> Report<V> {
        parent.into().with_child(self.into())
    }
}

/// Helper trait for [`Result<T, Report<E>>`].
pub trait ResultReportExt<T, E>: Sized {
    /// Converts the inner [`Report`] into a type implementing [`Error`].
    ///
    /// See [`Report::into_error`] for more information.
    fn into_error(self) -> CoreResult<T, impl Error + Send + Sync + 'static>;

    /// "Erases" this `Report`s generic parameter to `()`.
    ///
    /// See [`Report::erased`] for more information.
    fn erased(self) -> Result<T, ()>;

    /// "Wraps" the inner [`Report`] with the parent's default [`Report`].
    ///
    /// See [`Report::wrap`] for more information.
    #[track_caller]
    fn wrap<F: IntoReport + Default>(self) -> Result<T, F> {
        self.wrap_with_fn(|| F::default())
    }

    /// Append a [`Report`] as a parent of the inner [`Report`].
    ///
    /// See [`Report::wrap_with`] for more information.
    #[track_caller]
    fn wrap_with<F: IntoReport>(self, parent: F) -> Result<T, F> {
        self.wrap_with_fn(|| parent)
    }

    /// Append a [`Report`] as a parent of the inner [`Report`].
    ///
    /// See [`Report::wrap_with`] for more information.
    fn wrap_with_fn<F: IntoReport>(self, func: impl FnOnce() -> F) -> Result<T, F>;

    /// Sets the log level associated with the inner [`Report`].
    ///
    /// See [`Report::with_level`] for more information.
    fn with_level(self, level: Level) -> Result<T, E>;

    /// Appends a [`Frame`] to the inner [`Report`].
    ///
    /// See [`Report::with_frame`] for more information.
    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, E>;
}

impl<T, E, D: Into<Report<E>>> ResultReportExt<T, E> for CoreResult<T, D> {
    fn into_error(self) -> CoreResult<T, impl Error + Send + Sync + 'static> {
        self.map_err(|report| report.into().into_error())
    }

    fn erased(self) -> Result<T, ()> {
        self.map_err(|report| report.into().erased())
    }

    #[track_caller]
    fn wrap_with_fn<F: IntoReport>(self, parent: impl FnOnce() -> F) -> Result<T, F> {
        // we can't use [`Result::map_err`] since `#[track_caller]` on closures is unstable
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(parent().into_report().with_child(report.into())),
        }
    }

    fn with_level(self, level: Level) -> Result<T, E> {
        self.map_err(|report| report.into().with_level(level))
    }

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, E> {
        self.map_err(|report| report.into().with_frame(frame()))
    }
}

/// Trait for converting [`Error`]s into enriched [`Report`]s
///
/// This can be implemented via `#[derive(IntoReport)]`.
/// See [`xh_reports_derive::IntoReport`] for more information.
pub trait IntoReport: Sized + Send + Sync + 'static {
    /// Converts this Error into a [`Report`].
    ///
    /// Consumers should prefer `into_report` over [`Report::new`]
    /// to generate pre-enriched reports.
    ///
    /// Implementations should modify the [`Report`]
    /// (by adding frames, children, etc) as they see fit.
    #[track_caller]
    fn into_report(self) -> Report<Self>;
}

impl<E> IntoReport for E
where
    E: Error + 'static,
    E: Send + Sync,
{
    fn into_report(self) -> Report<Self> {
        Report::from_error(self)
    }
}

impl<T: IntoReport> From<T> for Report<T> {
    #[track_caller]
    fn from(value: T) -> Self {
        value.into_report()
    }
}

/// Helper struct for converting [`log::Record`]s into [`Report`]s.
///
/// This error can be converted into a [`Report`] via the [`IntoReport`] trait.
#[derive(Debug)]
pub struct LogError {
    message: SmolStr,
    level: Level,
    frames: Vec<Frame>,
    children: Vec<Report<LogSubError>>,
}

struct LogSubError;

impl LogError {
    /// Constructs a new [`LogError`]
    pub fn new(record: &log::Record) -> Self {
        #[derive(Default)]
        struct FrameVisitor {
            frames: Vec<Frame>,
            children: Vec<Report<LogSubError>>,
        }

        impl VisitSource<'_> for FrameVisitor {
            fn visit_pair(
                &mut self,
                key: Key<'_>,
                value: Value<'_>,
            ) -> CoreResult<(), log::kv::Error> {
                match key.as_str() {
                    "suggestion" => self.frames.push(Frame::suggestion(value.to_smolstr())),
                    "attachment" => self.frames.push(Frame::attachment(value)),
                    "error" => self.children.push(match value.to_borrowed_error() {
                        Some(error) => Report::from_error(error),
                        None => Report::<LogSubError>::new(value.to_smolstr()),
                    }),
                    key => self.frames.push(Frame::context(key, value)),
                };

                Ok(())
            }
        }

        let mut visitor = FrameVisitor::default();
        record.key_values().visit(&mut visitor).unwrap();
        visitor
            .frames
            .push(Frame::context("target", record.target()));

        Self {
            message: record.args().to_smolstr(),
            level: record.level(),
            children: visitor.children,
            frames: visitor.frames,
        }
    }
}

impl IntoReport for LogError {
    fn into_report(self) -> Report<Self> {
        Report::new(self.message)
            .with_frames(self.frames)
            .with_level(self.level)
            .with_children(self.children)
    }
}

/// Helper function to partition an [`Iterator`] based on its [`Result`]
pub fn partition_result<T, E>(
    iterator: impl Iterator<Item = CoreResult<T, E>>,
) -> CoreResult<Vec<T>, Vec<E>> {
    let mut ok = Vec::new();
    let mut err = Vec::new();

    iterator.for_each(|result| match result {
        Ok(v) => ok.extend(core::iter::once(v)),
        Err(v) => err.extend(core::iter::once(v)),
    });

    if err.is_empty() { Ok(ok) } else { Err(err) }
}

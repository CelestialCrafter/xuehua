//! Error handling crate designed for use with Xuehua

#![warn(missing_docs)]

pub mod prelude;
pub mod render;
#[cfg(feature = "tracing")]
pub mod tracing;

pub use xh_reports_derive::IntoReport;

use std::{
    any::type_name,
    error::Error,
    fmt::{self, Display},
    iter::once,
    marker::PhantomData,
    panic::Location as StdLocation,
    result::Result as StdResult,
};

use educe::Educe;
use smol_str::{SmolStr, ToSmolStr};

use crate::render::{GlobalRenderer, Renderer, SimpleRenderer};

/// Helper alias for [`Error`]
pub type BoxDynError = Box<dyn Error + Send + Sync + 'static>;

/// Helper alias for [`Result`](StdResult)
pub type Result<T, E> = StdResult<T, Report<E>>;

/// A single piece of information inside of a [`Report`].
///
/// `Frame`s can be created via the [`Self::suggestion`], [`Self::context`], or [`Self::attachment`] methods.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Frame {
    /// A collection of keys and values.
    ///
    /// This can be used to attach additional data such as:
    /// ids, timestamps, commands, etc.
    Context {
        /// Name of the context field
        key: SmolStr,
        /// Value associated with the key
        value: SmolStr,
    },
    /// A long-form piece of information.
    ///
    /// This can be used to attach more detailed data such as:
    /// stderr, build logs, files, etc.
    Attachment(SmolStr),
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
        Self::Context {
            key: key.into(),
            value: value.to_smolstr(),
        }
    }

    /// Helper function to create [`Self::Suggestion`]s.
    pub fn suggestion(suggestion: impl Into<SmolStr>) -> Frame {
        Self::Suggestion(suggestion.into())
    }

    /// Helper function to create [`Self::Attachment`]s.
    pub fn attachment(attachment: impl fmt::Display) -> Frame {
        Self::Attachment(attachment.to_smolstr())
    }
}

/// Location data associated with a [`Report`].
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Location {
    /// A location pointing to a file
    File {
        /// The name of the file
        name: SmolStr,
        /// Optionally, the line within the file
        line: Option<u32>,
        /// Optionally, the column within the line
        column: Option<u32>,
    },
    /// A location pointing to a module
    Module(SmolStr),
    /// An unknown locatiion
    #[default]
    Unknown,
}

/// Various error levels associated with a [`Report`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub enum Level {
    Trace,
    Debug,
    Warn,
    Info,
    Error,
}

/// Metadata associated with a [`Report`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Metadata {
    /// The location in which the [`Report`] originated.
    pub location: Location,
    /// The importance level of the [`Report`]
    pub level: Level,
    /// The name of the type associated with the [`Report`]
    ///
    /// Note: This field has the same semantics as [`type_name`]
    pub type_name: SmolStr,
}

impl Metadata {
    #[track_caller]
    fn new() -> Self {
        let std_location = StdLocation::caller();
        Self {
            location: Location::File {
                name: std_location.file().into(),
                line: Some(std_location.line()),
                column: Some(std_location.column()),
            },
            type_name: SmolStr::new_static(type_name::<()>()),
            level: Level::Error,
        }
    }
}

/// Inner payload of a [`Report`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportPayload {
    /// Frames attached to this report.
    pub frames: Vec<Frame>,
    /// Child reports that caused this report.
    pub children: Vec<ReportPayload>,
    /// Main message attached to this report.
    pub message: SmolStr,
    /// Other metadata associated with the report, such as creation location.
    pub metadata: Metadata,
}

impl ReportPayload {
    /// Construct a new `ReportPayload`.
    #[track_caller]
    pub fn new(message: SmolStr) -> Self {
        Self {
            message,
            metadata: Metadata::new(),
            children: Vec::default(),
            frames: Vec::default(),
        }
    }
}

/// Type representing a [`Report`], but implementing [`Error`].
///
/// This error type does not carry over all the "frills" of a report (such as pretty error trees).
/// It is also impossible to losslessly convert this back to a [`Report`].
#[derive(Debug)]
#[repr(transparent)]
pub struct ReportError(ReportPayload);

impl Error for ReportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        (self.0.children.len() == 1).then(|| {
            let child = &self.0.children[0];
            // SAFETY: `ReportError` is repr(transparent) over `ReportPayload`.
            let error = unsafe { std::mem::transmute::<&ReportPayload, &Self>(child) };
            error as &(dyn Error + 'static)
        })
    }
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        SimpleRenderer.render(&self.0).fmt(f)
    }
}

/// The core error type, representing a tree of errors.
///
/// Each report contains [`Frame`]s, child [`ReportPayload`]s,
/// and additional information about the error.
#[derive(Educe, Clone)]
#[educe(Deref, DerefMut)]
pub struct Report<E> {
    #[educe(Deref, DerefMut)]
    inner: Box<ReportPayload>,
    _marker: PhantomData<fn() -> E>,
}

/// The [`fmt::Debug`] impl for this type defaults to rendering using the [`GlobalRenderer`].
/// Additionally, the `{:#?}` directive can show the `Report` in a typical [`fmt::Debug`] fashion.
impl<E> fmt::Debug for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            self.inner.fmt(f)
        } else {
            GlobalRenderer.render(self).fmt(f)
        }
    }
}

impl<E> fmt::Display for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        SimpleRenderer.render(self).fmt(f)
    }
}

impl Report<()> {
    /// Constructs a new [`Report`] from a message.
    #[track_caller]
    pub fn new(message: impl Into<SmolStr>) -> Self {
        Self::from_payload(ReportPayload::new(message.into()))
    }

    /// Converts the underlying [`ReportPayload`] back into a `Report`.
    pub fn from_payload(payload: ReportPayload) -> Self {
        Report {
            inner: payload.into(),
            _marker: PhantomData,
        }
    }

    /// Constructs a new [`Report`] from an error.
    ///
    /// Note that this walks [`Error::source`] to build a tree of children.
    /// To avoid this behavior, consider using [`Report::new`]
    #[track_caller]
    pub fn from_error(error: impl Error) -> Self {
        #[track_caller]
        fn walk(error: &dyn Error) -> Report<()> {
            let report = Report::new(error.to_smolstr());
            match error.source() {
                Some(source) => report.with_child(walk(source)),
                None => report,
            }
        }

        walk(&error)
    }
}

impl<E> Report<E> {
    /// Converts the `Report` into the underlying [`ReportPayload`].
    pub fn into_payload(self) -> ReportPayload {
        *self.inner
    }

    /// Converts this `Report` into a [`ReportError`]
    pub fn into_error(self) -> ReportError {
        ReportError(*self.inner)
    }

    /// "Erases" this `Report`s generic parameter to [`()`].
    // NOTE: This should not change self.inner.metadata.type_name like `Self::cast` does.
    pub fn erased(self) -> Report<()> {
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }

    /// Sets the log level associated with the `Report`.
    pub fn with_level(mut self, level: Level) -> Self {
        self.inner.metadata.level = level;
        self
    }

    /// Appends a [`Frame`] to this `Report`.
    pub fn with_frame(mut self, frame: Frame) -> Self {
        self.inner.frames.push(frame);
        self
    }

    /// Appends an iterator of [`Frame`]s to this `Report`.
    pub fn with_frames(self, frames: impl IntoIterator<Item = Frame>) -> Self {
        frames.into_iter().fold(self, Report::with_frame)
    }

    /// Appends a `Report` as a child to this `Report`.
    pub fn with_child<F>(mut self, child: Report<F>) -> Self {
        self.inner.children.push(*child.inner);
        self
    }

    /// Appends an iterator of `Report`s as children of this `Report`.
    pub fn with_children<F>(self, children: impl IntoIterator<Item = Report<F>>) -> Self {
        children.into_iter().fold(self, Report::with_child)
    }

    /// "Casts" the reports generic parameter to `F`.
    pub fn cast<F>(mut self) -> Report<F> {
        self.inner.metadata.type_name = SmolStr::new_static(type_name::<F>());
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }
}

/// Helper trait for [`Report<T>`].
pub trait ReportExt<T> {
    /// "Wraps" this `Report` with the parent's default `Report`.
    ///
    /// See [`Report::wrap`] for more information.
    #[track_caller]
    fn wrap<U: IntoReport + Default>(self) -> Report<U>;

    /// "Wraps" this `Report` with a parent `Report`.
    ///
    /// See [`Report::wrap_with`] for more information.
    #[track_caller]
    fn wrap_with<U>(self, parent: impl Into<Report<U>>) -> Report<U>;
}

impl<T, U: Into<Report<T>>> ReportExt<T> for U {
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
    #[track_caller]
    fn into_error(self) -> StdResult<T, impl Error + Send + Sync + 'static>;

    /// "Erases" this `Report`s generic parameter to `()`.
    ///
    /// See [`Report::erased`] for more information.
    #[track_caller]
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
    fn wrap_with<F, G: Into<Report<F>>>(self, parent: G) -> Result<T, F> {
        self.wrap_with_fn(|| parent)
    }

    /// Append a [`Report`] as a parent of the inner [`Report`].
    ///
    /// See [`Report::wrap_with`] for more information.
    #[track_caller]
    fn wrap_with_fn<F, G: Into<Report<F>>>(self, func: impl FnOnce() -> G) -> Result<T, F>;

    /// Sets the log level associated with the inner [`Report`].
    ///
    /// See [`Report::with_level`] for more information.
    #[track_caller]
    fn with_level(self, level: Level) -> Result<T, E>;

    /// Appends a [`Frame`] to the inner [`Report`].
    ///
    /// See [`Report::with_frame`] for more information.
    #[track_caller]
    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, E>;
}

// we can't use [`Result::map_err`] in any of these since `#[track_caller]` on closures is unstable
impl<T, E, D: Into<Report<E>>> ResultReportExt<T, E> for StdResult<T, D> {
    fn into_error(self) -> StdResult<T, impl Error + Send + Sync + 'static> {
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(report.into().into_error()),
        }
    }

    fn erased(self) -> Result<T, ()> {
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(report.into().erased()),
        }
    }

    fn wrap_with_fn<F, G: Into<Report<F>>>(self, parent: impl FnOnce() -> G) -> Result<T, F> {
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(parent().into().with_child(report.into())),
        }
    }

    fn with_level(self, level: Level) -> Result<T, E> {
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(report.into().with_level(level)),
        }
    }

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Result<T, E> {
        match self {
            Ok(t) => Ok(t),
            Err(report) => Err(report.into().with_frame(frame())),
        }
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
        Report::from_error(self).cast()
    }
}

impl<T: IntoReport> From<T> for Report<T> {
    #[track_caller]
    fn from(value: T) -> Self {
        value.into_report()
    }
}

/// Defines a Compat trait for populating common error types.
#[macro_export]
macro_rules! impl_compat {
    ($name:ident, $(($error:path, |$argument:ident| $block:expr)),*) => {
        /// Helper trait for populating common error types.
        pub trait $name<T, E>: Sized {
            /// Converts this error into a pre-populated [`Report`]($crate::Report).
            fn compat(self) -> ::std::result::Result<T, $crate::Report<E>>;
        }

        $(impl<T> $name<T, $error> for ::std::result::Result<T, $error> {
            #[track_caller]
            fn compat(self) -> ::std::result::Result<T, $crate::Report<$error>> {
                #[track_caller]
                fn convert($argument: $error) -> $crate::Report<$error> {
                    $block
                }

                // we can't use [`Result::map_err`] since `#[track_caller]` doesn't work with it
                match self {
                    Ok(t) => Ok(t),
                    Err(e) => Err(convert(e))
                }
            }
        })*
    };
}

/// Helper function to partition an [`Iterator`] based on its [`Result`]
pub fn partition_results<T, U, E, F>(
    iterator: impl Iterator<Item = StdResult<T, E>>,
) -> StdResult<U, F>
where
    U: Extend<T> + Default,
    F: Extend<E> + Default,
{
    let mut ok = U::default();
    let mut err = F::default();

    let mut has_error = false;
    iterator.for_each(|result| match result {
        Ok(v) => ok.extend(once(v)),
        Err(v) => {
            has_error = true;
            err.extend(once(v));
        }
    });

    if has_error { Err(err) } else { Ok(ok) }
}

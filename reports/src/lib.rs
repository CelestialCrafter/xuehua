// TODO: #![warn(missing_docs)]

#![no_std]

extern crate alloc;

pub use xh_reports_derive::IntoReport;
pub mod render;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use core::{any::type_name, error::Error, fmt, marker::PhantomData, panic::Location};

use educe::Educe;
use log::{
    Level,
    kv::{Key, Value, VisitSource},
};
use smallvec::SmallVec;
use smol_str::{SmolStr, ToSmolStr};
use thiserror::Error;

use crate::render::{Render, SimpleRenderer};

pub type BoxDynError = Box<dyn Error + Send + Sync + 'static>;

#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Context(Vec<(SmolStr, String)>),
    Attachment(String),
    Suggestion(SmolStr),
}

impl Frame {
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

    pub fn suggestion(suggestion: impl Into<SmolStr>) -> Frame {
        Self::Suggestion(suggestion.into())
    }

    pub fn attachment(attachment: impl fmt::Display) -> Frame {
        Self::Attachment(attachment.to_string())
    }
}

#[derive(Debug)]
struct ReportInner {
    frames: SmallVec<[Frame; 1]>,
    children: SmallVec<[Report<()>; 1]>,
    error: BoxDynError,
    type_name: &'static str,
    location: &'static Location<'static>,
    level: Level,
}

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
    #[track_caller]
    pub fn new(error: E) -> Self
    where
        E: Error + 'static,
        E: Send + Sync,
    {
        fn walk(
            error: &dyn Error,
            location: &'static Location<'static>,
        ) -> SmallVec<[Report<()>; 1]> {
            let mut reports = SmallVec::new();
            if let Some(source) = error.source() {
                reports.push(Report {
                    inner: Box::new(ReportInner {
                        children: walk(source, location),
                        error: Box::new(SourceError(source.to_string())),
                        frames: Default::default(),
                        type_name: type_name::<SourceError>(),
                        location,
                        level: Level::Error,
                    }),
                    _marker: PhantomData,
                });
            }

            reports
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

    pub fn error(&self) -> &(dyn Error + Send + Sync) {
        &*self.inner.error
    }

    pub fn type_name(&self) -> &'static str {
        self.inner.type_name
    }

    pub fn location(&self) -> &'static Location<'static> {
        self.inner.location
    }

    pub fn erased(self) -> Report<()> {
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }

    pub fn with_level(mut self, level: Level) -> Self {
        self.inner.level = level;
        self
    }

    pub fn level(&self) -> Level {
        self.inner.level
    }

    pub fn frames(&self) -> &[Frame] {
        &self.inner.frames
    }

    pub fn push_frame(&mut self, frame: Frame) {
        self.inner.frames.push(frame);
    }

    pub fn with_frame(mut self, frame: Frame) -> Self {
        self.push_frame(frame);
        self
    }

    pub fn with_frames(self, frames: impl IntoIterator<Item = Frame>) -> Self {
        frames.into_iter().fold(self, |acc, x| acc.with_frame(x))
    }

    pub fn children(&self) -> &[Report<()>] {
        &self.inner.children
    }

    pub fn push_child<F>(&mut self, child: Report<F>) {
        self.inner.children.push(child.erased());
    }

    pub fn with_child<F>(mut self, child: Report<F>) -> Self {
        self.push_child(child);
        self
    }

    pub fn with_children<F>(self, children: impl IntoIterator<Item = Report<F>>) -> Self {
        children.into_iter().fold(self, |acc, x| acc.with_child(x))
    }
}

impl<E> fmt::Display for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        SimpleRenderer.render(self).fmt(f)
    }
}

impl<E> Error for Report<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        let children = &self.inner.children;
        (1 == children.len()).then(|| children.first().unwrap() as _)
    }
}

pub trait ResultReportExt<T> {
    fn erased(self) -> Result<T, Report<()>>;

    fn wrap<F>(self, error: impl FnOnce() -> Report<F>) -> Result<T, Report<F>>;

    fn with_level(self, level: Level) -> Self;

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Self;
}

impl<T, E> ResultReportExt<T> for Result<T, Report<E>> {
    fn erased(self) -> Result<T, Report<()>> {
        self.map_err(|report| report.erased())
    }

    fn wrap<F>(self, error: impl FnOnce() -> Report<F>) -> Result<T, Report<F>> {
        self.map_err(|report| error().with_child(report))
    }

    fn with_level(self, level: Level) -> Self {
        self.map_err(|report| report.with_level(level))
    }

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Self {
        self.map_err(|report| report.with_frame(frame()))
    }
}

pub trait IntoReport: Sized + Error + Send + Sync + 'static {
    #[track_caller]
    fn into_report(self) -> Report<Self> {
        Report::new(self)
    }
}

#[derive(Error, Debug)]
#[error("{message}")]
pub struct LogError {
    message: String,
    level: Level,
    children: SmallVec<[Report<()>; 0]>,
    frames: SmallVec<[Frame; 0]>,
}

#[derive(Error, Debug)]
#[error("{0}")]
struct LogSubError(String);

impl LogError {
    pub fn new(record: &log::Record) -> Self {
        #[derive(Default)]
        struct FrameVisitor {
            frames: SmallVec<[Frame; 0]>,
            children: SmallVec<[Report<()>; 0]>,
            context: SmallVec<[(SmolStr, String); 2]>,
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

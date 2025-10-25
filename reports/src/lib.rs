// TODO: create panic hook
// TODO: #![warn(missing_docs)]

#![no_std]

extern crate alloc;

use thiserror::Error;
use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use smol_str::SmolStr;
use core::{any::type_name, error::Error, fmt, marker::PhantomData, panic::Location};
use educe::Educe;
use smallvec::SmallVec;

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
}

#[derive(Educe)]
#[educe(Debug(bound()))]
pub struct Report<E> {
    inner: Box<ReportInner>,
    _marker: PhantomData<E>,
}

impl<E> Report<E> {
    #[track_caller]
    pub fn new(error: E) -> Self
    where
        E: Error + 'static,
        E: Send + Sync,
    {
        #[derive(Error, Debug)]
        #[error("{0}")]
        struct SourceError(String);

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

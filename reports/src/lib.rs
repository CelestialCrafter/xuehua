// TODO: create panic hook
// TODO: #![warn(missing_docs)]

#![no_std]

extern crate alloc;

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
        let location = Location::caller();
        Self {
            inner: Box::new(ReportInner {
                error: Box::new(error),
                children: SmallVec::new(),
                frames: Default::default(),
                type_name: type_name::<E>(),
                location,
            }),
            _marker: PhantomData,
        }
    }
}

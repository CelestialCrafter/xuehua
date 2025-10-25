pub mod render;

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::{String, ToString},
};
use core::panic::Location;
use core::{
    error::Error,
    fmt::{self, Display},
    marker::PhantomData,
};
use smallvec::SmallVec;
use thiserror::Error;

use smol_str::SmolStr;

use crate::render::{HumanRenderer, JsonRenderer, Renderer};
extern crate alloc;

pub type BoxDynError = Box<dyn Error + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Location(&'static Location<'static>),
    Context(Vec<(SmolStr, String)>),
    Attachment(String),
    Suggestion(Cow<'static, str>),
}

impl Frame {
    pub fn context<K, V, I>(context: I) -> Self
    where
        K: Into<SmolStr>,
        V: Display,
        I: IntoIterator<Item = (K, V)>,
    {
        let context = context
            .into_iter()
            .map(|(key, value): (K, V)| (key.into(), value.to_string()))
            .collect();
        Self::Context(context)
    }

    pub fn suggestion(suggestion: impl Into<Cow<'static, str>>) -> Frame {
        Self::Suggestion(suggestion.into())
    }

    pub fn attachment(attachment: impl Display) -> Frame {
        Self::Attachment(attachment.to_string())
    }

    #[inline]
    #[track_caller]
    pub fn location() -> Frame {
        Frame::Location(Location::caller())
    }
}

#[derive(Debug)]
struct ReportInner {
    pub(crate) frames: SmallVec<[Frame; 1]>,
    pub(crate) children: SmallVec<[Report<()>; 1]>,
    pub(crate) error: BoxDynError,
}

#[derive(Debug)]
pub struct Report<E> {
    pub(crate) inner: Box<ReportInner>,
    _marker: PhantomData<E>,
}

impl<E> Report<E>
where
    E: Error + 'static,
    E: Send + Sync,
{
    #[track_caller]
    pub fn new(error: E) -> Self {
        #[derive(Error, Debug)]
        #[error("{0}")]
        struct SourceError(String);

        fn walk(
            error: &dyn Error,
            location: &'static Location<'static>,
        ) -> SmallVec<[Report<()>; 1]> {
            let mut reports = SmallVec::new();

            if let Some(source) = error.source() {
                reports.push(
                    Report {
                        inner: Box::new(ReportInner {
                            children: walk(source, location),
                            error: Box::new(SourceError(source.to_string())),
                            frames: Default::default(),
                        }),
                        _marker: PhantomData,
                    }
                    .with_frame(Frame::Location(location)),
                );
            }

            reports
        }

        let location = Location::caller();
        Self {
            inner: Box::new(ReportInner {
                children: walk(&error, location),
                error: Box::new(error),
                frames: Default::default(),
            }),
            _marker: PhantomData,
        }
        .with_frame(Frame::Location(location))
    }
}

impl<E> Report<E> {
    pub fn error(&self) -> &BoxDynError {
        &self.inner.error
    }

    pub fn erased(self) -> Report<()> {
        Report {
            inner: self.inner,
            _marker: PhantomData,
        }
    }

    pub fn frames(&self) -> impl Iterator<Item = &Frame> {
        self.inner.frames.iter()
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

    pub fn children(&self) -> impl Iterator<Item = &BoxDynError> {
        self.inner.children.iter().map(|child| &child.inner.error)
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

impl<E: fmt::Debug> Error for Report<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        let children = &self.inner.children;
        (1 == children.len()).then(|| children.first().unwrap() as _)
    }
}

pub trait ResultReportExt<T> {
    fn erased(self) -> Result<T, Report<()>>;

    fn wrap<F>(self, error: impl FnOnce() -> Report<F>) -> Result<T, Report<F>>;

    fn with_frame(self, frame: impl FnOnce() -> Frame) -> Self;
}

impl<T, E> ResultReportExt<T> for Result<T, Report<E>> {
    fn erased(self) -> Result<T, Report<()>> {
        self.map_err(|report| report.erased())
    }

    fn wrap<F>(self, error: impl FnOnce() -> Report<F>) -> Result<T, Report<F>> {
        self.map_err(|report| error().with_child(report))
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

#[cfg(test)]
mod tests {
    use std::num::TryFromIntError;

    use smol_str::SmolStr;
    use thiserror::Error;

    use crate::{Frame, IntoReport, Report, ResultReportExt};

    #[test]
    fn test_source_conversion() {
        #[derive(Error, Debug)]
        #[error("sub error")]
        struct SubError;

        #[derive(Error, Debug)]
        enum TestError {
            #[error(transparent)]
            SubError(#[from] SubError),
        }

        let report = Report::new(TestError::SubError(SubError));
        let child = report
            .inner
            .children
            .get(0)
            .expect("error should have child");

        assert!(
            child.inner.error.is::<SubError>(),
            "child error was not SubError"
        )
    }

    #[test]
    fn test_stuff() {
        #[derive(Error, Debug)]
        #[error("could not build program")]
        struct BuildError;

        #[derive(Error, Debug)]
        #[error("archive download failed")]
        struct DownloadError {
            url: &'static str,
            method: &'static str,
        }

        impl IntoReport for DownloadError {
            fn into_report(self) -> Report<Self> {
                let frames = [
                    Frame::context([("url", self.url), ("method", self.method)]),
                    Frame::suggestion("try downloading a valid archive"),
                    Frame::attachment(
                        "uh we tried to do some\nstuff but it didnt\nreally work out!!",
                    ),
                ];

                Report::new(self).with_frames(frames)
            }
        }

        #[derive(Error, Debug)]
        #[error("unexpected token encountered")]
        struct UnexpectedTokenError {
            token: SmolStr,
            expected: String,
        }

        impl IntoReport for UnexpectedTokenError {
            fn into_report(self) -> Report<Self> {
                let frames = [
                    Frame::suggestion(format!("try providing {} instead", self.expected)),
                    Frame::context([("token", self.token.clone())]),
                ];

                Report::new(self).with_frames(frames)
            }
        }

        #[derive(Error, Debug)]
        #[error("could not decode archive")]
        struct DecodingError;

        impl IntoReport for DecodingError {}

        fn try_conv(value: u64) -> Result<(), Report<TryFromIntError>> {
            match u32::try_from(value) {
                Ok(_) => Ok(()),
                Err(err) => Err(Report::new(err)
                    .with_frame(Frame::context([("value", value)]))
                    .with_frame(Frame::suggestion("try giving a smaller number"))),
            }
        }

        fn try_decode_gups(token: SmolStr) -> Result<(), Report<UnexpectedTokenError>> {
            if token != "gupgupgup" {
                return Err(UnexpectedTokenError {
                    token,
                    expected: format!("3 gups"),
                }
                .into_report());
            }

            Ok(())
        }

        fn try_decode_archive(aura: bool) -> Result<(), Report<DecodingError>> {
            if aura {
                try_conv(u64::MAX).erased()
            } else {
                try_decode_gups("gup".into()).erased()
            }
            .wrap(|| DecodingError.into_report())
        }

        fn try_download() -> Result<Vec<()>, Vec<Report<DownloadError>>> {
            let (ok, err) = [
                "https://celestial.moe/my-page",
                "https://celestial.moe/my-other-page",
                "https://celestial.moe/wauwaaa",
            ]
            .into_iter()
            .enumerate()
            .map(|(i, url)| {
                try_decode_archive(i % 2 == 0).wrap(|| {
                    DownloadError {
                        url,
                        method: "GET".into(),
                    }
                    .into_report()
                })
            })
            .fold((Vec::new(), Vec::new()), |mut acc, x| {
                match x {
                    Ok(value) => acc.0.push(value),
                    Err(error) => acc.1.push(error),
                }

                acc
            });

            if err.is_empty() { Ok(ok) } else { Err(err) }
        }

        fn try_build() -> Result<(), Report<BuildError>> {
            match try_download() {
                Ok(_) => Ok(()),
                Err(reports) => Err(Report::new(BuildError).with_children(reports)),
            }
        }

        if let Err(err) = try_build() {
            eprintln!("{err}");
            panic!();
        }
    }
}

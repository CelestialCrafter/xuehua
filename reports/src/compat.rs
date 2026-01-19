//! Report population for common error types.

use crate::prelude::*;

/// Defines a compat trait.
#[macro_export]
macro_rules! impl_compat {
    ($name:ident, $(($error:path, |$argument:ident| $block:expr)),*) => {
        /// Helper trait for populating common errors.
        pub trait $name<T, E>: Sized {
            /// Converts this error into a pre-populated [`Report`]($crate::Report).
            fn compat(self) -> ::core::result::Result<T, $crate::Report<E>>;
        }

        $(impl<T> $name<T, $error> for ::core::result::Result<T, $error> {
            #[track_caller]
            fn compat(self) -> ::core::result::Result<T, $crate::Report<$error>> {
                #[track_caller]
                fn convert($argument: $error) -> $crate::Report<$error> {
                    $block
                }

                // we can't use [`Result::map_err`] since `#[track_caller]` on closures is unstable
                match self {
                    Ok(t) => Ok(t),
                    Err(e) => Err(convert(e))
                }
            }
        })*
    };
}

impl_compat!(
    StdCompat,
    (std::io::Error, |error| {
        use std::io::ErrorKind;

        let mut frames = std::vec::Vec::new();
        match error.kind() {
            ErrorKind::NotFound => {
                frames.push(Frame::suggestion("provide a file that exists"));
            }
            ErrorKind::PermissionDenied => {
                frames.push(Frame::suggestion(
                    "provide a resource with the appropriate permissions",
                ));
            }
            ErrorKind::AlreadyExists => {
                frames.push(Frame::suggestion("provide a path with non-existent files"));
            }
            ErrorKind::DirectoryNotEmpty => {
                frames.push(Frame::suggestion("provide an empty directory"));
            }
            _ => (),
        }

        Report::from_error(error).with_frames(frames)
    })
);

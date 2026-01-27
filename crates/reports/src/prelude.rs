//! Common imports for `xh-reports`

pub use crate::{BoxDynError, Frame, IntoReport, Report, ReportExt, Result, ResultReportExt};
#[cfg(feature = "std")]
pub use std::result::Result as StdResult;
#[cfg(not(feature = "std"))]
pub use core::result::Result as CoreResult;

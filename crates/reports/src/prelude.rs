//! Common imports for `xh-reports`

pub use std::{result::Result as StdResult, error::Error as StdError};
pub use crate::{BoxDynError, Frame, IntoReport, Report, ReportExt, Result, ResultReportExt};

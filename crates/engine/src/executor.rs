use xh_reports::prelude::*;

use crate::name::ExecutorName;

pub trait Executor: Send + Sized {
    type Request: serde::de::DeserializeOwned;
    type Error: IntoReport;

    fn name() -> &'static ExecutorName;

    fn execute(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

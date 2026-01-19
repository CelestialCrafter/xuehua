use xh_reports::prelude::*;

pub trait Executor: Send + Sized {
    const NAME: &'static str;
    type Request: serde::de::DeserializeOwned;
    type Error: IntoReport;

    fn execute(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

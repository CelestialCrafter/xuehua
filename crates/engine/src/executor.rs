use xh_reports::prelude::*;

use crate::name::ExecutorName;

#[derive(Default, Debug, IntoReport)]
#[message("could not run executor action")]
pub struct Error;

pub trait Executor: Send + Sized {
    type Request: serde::de::DeserializeOwned;

    fn name() -> &'static ExecutorName;

    fn execute(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

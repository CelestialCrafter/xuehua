pub mod runner;

#[cfg(feature = "bubblewrap-executor")]
pub use runner::bubblewrap;

use crate::utils::BoxDynError;

pub const MODULE_NAME: &str = "xuehua.executor";

// TODO: add examples for executor implementation and usage
/// A controlled gateway for executing side-effects of a package build
///
/// An [`Executor`] is the bridge between an isolated and pure [`Package`](crate::package::Package) definition,
/// and messy real-world actions package builds need to do.
/// Its responsibility is to provide a secure, isolated, and reproducable environment for package builds to actually do things.
///
/// By nature, executors are full of side effects (fetching data, running processes, creating files, etc),
/// but they must strive to be deterministic.
pub trait Executor: Sized {
    type Request;
    type Response;

    fn dispatch(
        &mut self,
        request: Self::Request,
    ) -> impl Future<Output = Result<Self::Response, BoxDynError>> + Send;
}

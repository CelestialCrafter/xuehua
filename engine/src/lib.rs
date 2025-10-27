//! This crate contains the engine for the Xuehua build system/package manager

pub mod package;
pub mod builder;
pub mod executor;
pub mod scheduler;
pub mod store;
pub mod planner;
pub mod logger;
pub mod utils;

impl_into_err!(
    (store::Error, into_store_err)
);

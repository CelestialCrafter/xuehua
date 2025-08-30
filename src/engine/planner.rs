use thiserror::Error;

use crate::engine::Id;

#[derive(Error, Debug)]
pub enum PlanError {
    #[error("package not found: {package}")]
    NotFound { package: Id },
    #[error("conflicting package: {package}")]
    Conflict { package: Id },
    #[error("cycle detected from {from} to {to}")]
    Cyclic { from: Id, to: Id },
}

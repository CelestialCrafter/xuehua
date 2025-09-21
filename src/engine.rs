pub mod builder;
pub mod logger;
pub mod package;
pub mod planner;
pub mod utils;

use std::path::Path;

use mlua::Lua;
use petgraph::dot::Dot;
use thiserror::Error;

use crate::{
    engine::planner::{Planner, PlannerError},
    utils::LuaError,
};

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("error running planner")]
    PlannerError(
        #[source]
        #[from]
        PlannerError,
    ),
    #[error("injection failed for {api}")]
    InjectionFailed {
        api: String,
        #[source]
        error: LuaError,
    },
}

fn into_injection<T>(api: &str, result: Result<T, mlua::Error>) -> Result<T, EngineError> {
    result.map_err(|err| EngineError::InjectionFailed {
        api: api.to_string(),
        error: err.into(),
    })
}

pub fn run(root: &Path) -> Result<(), EngineError> {
    // TODO: restrict stdlibs
    let lua = Lua::new();

    // inject apis
    into_injection("logger", logger::inject(&lua))?;
    into_injection("utils", utils::inject(&lua))?;

    // execute lua
    let planner = Planner::run(&lua, root)?;
    println!("{:?}", Dot::new(&planner.plan()));

    Ok(())
}

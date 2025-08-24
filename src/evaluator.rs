use std::ops::Deref;

use eyre::{eyre, Report};

#[derive(Debug)]
pub enum EvaluationError {
    LuaError(Report),
    InstructionError(Report),
    NotFound(Report),
    Conflict(Report),
    Other(Report),
}

impl Deref for EvaluationError {
    type Target = Report;

    fn deref(&self) -> &Self::Target {
        match self {
            EvaluationError::LuaError(err) => err,
            EvaluationError::InstructionError(err) => err,
            EvaluationError::NotFound(err) => err,
            EvaluationError::Conflict(err) => err,
            EvaluationError::Other(err) => err,
        }
    }
}

impl From<mlua::Error> for EvaluationError {
    fn from(value: mlua::Error) -> Self {
        EvaluationError::LuaError(eyre!(value.to_string()))
    }
}

impl Into<mlua::Error> for EvaluationError {
    fn into(self) -> mlua::Error {
        mlua::Error::runtime(self.to_string())
    }
}

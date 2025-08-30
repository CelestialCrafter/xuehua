use std::sync::Arc;

use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct LuaError(Arc<mlua::Error>);

// SAFETY: we only ever read mlua::Error thru Arc immutably, so it's fine to impl Send/Sync
unsafe impl Send for LuaError {}
unsafe impl Sync for LuaError {}

impl From<mlua::Error> for LuaError {
    fn from(value: mlua::Error) -> Self {
        Self(Arc::new(value))
    }
}

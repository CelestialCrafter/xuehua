pub mod executor;
pub mod logger;
pub mod package;
pub mod planner;
pub mod utils;

use std::{collections::HashMap, rc::Rc};

use mlua::Lua;
use radix_trie::TrieCommon;
use thiserror::Error;

use crate::utils::LuaError;

pub struct APIGuard<'a, A> {
    strong: Rc<A>,
    lua: &'a Lua,
}

#[macro_export]
macro_rules! impl_inject_api {
    ($api:ident, $finalized:ident, $module:expr, $(($fn:ident, $lua:expr),)*) => {
        impl<'a> APIGuard<'a, $api> {
            pub fn inject(lua: &'a Lua) -> Result<Self, mlua::Error> {
                let strong = Rc::new($api::default());
                let weak = Rc::downgrade(&strong);

                let module = lua.create_table()?;

                $({
                    let weak = weak.clone();
                    module.set($lua, lua.create_function(move |lua, values| {
                        weak.upgrade()
                            .ok_or(PlanError::ModuleRestricted($module.to_string()))
                            .into_lua_err()?
                            .$fn(lua, values)
                    })?)?;
                })*

                lua.register_module($module, module)?;

                Ok(Self { strong, lua })
            }

            pub fn release(mut self) -> Result<$finalized, mlua::Error> {
                let inner = Rc::try_unwrap(std::mem::take(&mut self.strong))
                    .map_err(|_| "only one strong reference to the api should exist")
                    .unwrap();


                self.lua.unload_module($module)?;
                Ok(inner.into_inner())
            }
        }
    };
}

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("lua runtime error")]
    LuaError(#[source] LuaError),
    #[error("injection failed for {api}")]
    InjectionFailed {
        api: String,
        #[source]
        error: LuaError,
    },
}

impl From<mlua::Error> for EngineError {
    fn from(err: mlua::Error) -> Self {
        EngineError::LuaError(err.into())
    }
}

fn convert_err<T>(api: &str, result: Result<T, mlua::Error>) -> Result<T, EngineError> {
    result.map_err(|err| EngineError::InjectionFailed {
        api: api.to_string(),
        error: err.into(),
    })
}

pub fn run(source: &[u8]) -> Result<(), EngineError> {
    // TODO: restrict stdlibs
    let lua = Lua::new();

    // inject apis
    convert_err("logger", logger::inject(&lua))?;
    convert_err("utils", utils::inject(&lua))?;
    let plan = convert_err("plan", APIGuard::inject(&lua))?;

    // execute lua
    lua.load(source).exec()?;
    let plan = plan.release()?;
    let plan: HashMap<_, _> = plan.packages.iter().collect();
    dbg!(plan);

    Ok(())
}

pub mod executor;
pub mod planner;

use std::{collections::HashMap, rc::Rc};

use mlua::{FromLua, Function, Lua, LuaSerdeExt, Table, Value};
use radix_trie::TrieCommon;
use serde::Deserialize;
use thiserror::Error;

use crate::engine::{executor::ExecError, planner::PlanError};

pub type PackageId = String;

#[derive(Deserialize, Debug)]
pub struct PackageMetadata {}

#[derive(Debug)]
pub struct Package {
    pub id: PackageId,
    pub dependencies: Vec<PackageId>,
    pub metadata: PackageMetadata,
    // TODO: make this field private, and then create a wrapper function
    pub build: Function,
}

impl FromLua for Package {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self, mlua::Error> {
        let table = Table::from_lua(value, lua)?;

        Ok(Self {
            id: table.get("id")?,
            dependencies: lua.from_value(table.get("dependencies")?)?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
        })
    }
}

pub struct APIGuard<A> {
    strong: Rc<A>,
    lua: Lua,
}

#[macro_export]
macro_rules! impl_inject_api {
    ($api:ident, $finalized:ident, $module:expr, $(($fn:ident, $lua:expr),)*) => {
        impl APIGuard<$api> {
            pub fn inject(lua: Lua) -> Result<Self, mlua::Error> {
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
    #[error("planning error")]
    Plan(
        #[from]
        #[source]
        PlanError,
    ),
    #[error("executor error")]
    Executor(
        #[from]
        #[source]
        ExecError,
    ),
}

pub fn run(source: &[u8]) -> Result<(), EngineError> {
    // TODO: restrict stdlibs
    let lua = Lua::new();

    let run_plan = || {
        let api = APIGuard::inject(lua.clone())?;
        lua.load(source).exec()?;
        Ok::<_, PlanError>(api.release()?)
    };

    let plan = run_plan()?;
    let plan: HashMap<_, _> = plan.packages.iter().collect();
    dbg!(plan);

    Ok(())
}

use std::{
    cell::{BorrowMutError, RefCell},
    rc::Rc,
};

use mlua::{ExternalResult, Function, Lua};
use radix_trie::Trie;
use thiserror::Error;

use crate::{
    engine::{APIGuard, Package, PackageId},
    impl_inject_api,
    utils::LuaError,
};

#[derive(Error, Debug)]
pub enum PlanError {
    #[error("package not found: {package}")]
    NotFound { package: PackageId },
    #[error("conflicting package: {package}")]
    Conflict { package: PackageId },
    #[error("cycle detected from {from} to {to}")]
    Cyclic { from: PackageId, to: PackageId },
    #[error("module {0} restricted in current scope")]
    ModuleRestricted(String),
    #[error("nested calls to {func} are not allowed")]
    NestedCall {
        func: String,
        #[source]
        error: BorrowMutError,
    },
    #[error("lua runtime error")]
    LuaError(#[source] LuaError),
}

impl From<mlua::Error> for PlanError {
    fn from(err: mlua::Error) -> Self {
        PlanError::LuaError(err.into())
    }
}

#[derive(Debug)]
pub struct Plan {
    pub packages: Trie<PackageId, Package>,
}
#[derive(Default)]
struct PlanPackages {
    concrete: Trie<PackageId, Package>,
}

#[derive(Default)]
pub struct PlanAPI {
    packages: RefCell<PlanPackages>,
    crumbs: RefCell<Vec<String>>,
}

impl PlanAPI {
    fn package(&self, _lua: &Lua, mut pkg: Package) -> Result<String, mlua::Error> {
        let mut crumbs = self.crumbs.borrow_mut();
        crumbs.push(pkg.id);
        pkg.id = crumbs.join("/");
        crumbs.pop();

        let name = pkg.id.clone();
        let mut planner_packages = self
            .packages
            .try_borrow_mut()
            .map_err(|err| PlanError::NestedCall {
                func: "package".to_string(),
                error: err,
            })
            .into_lua_err()?;

        match planner_packages.concrete.insert(name.clone(), pkg) {
            Some(conflicting) => Err(PlanError::Conflict {
                package: conflicting.id,
            })
            .into_lua_err(),
            None => Ok(name),
        }
    }

    fn group(
        &self,
        _lua: &Lua,
        (crumb, closure): (String, Function),
    ) -> Result<String, mlua::Error> {
        let mut crumbs = self.crumbs.borrow_mut();
        crumbs.push(crumb);
        let joined = crumbs.join("/");
        drop(crumbs);

        closure.call::<()>(joined.clone())?;

        self.crumbs.borrow_mut().pop();
        Ok(joined)
    }

    fn into_inner(self) -> Plan {
        Plan {
            packages: self.packages.into_inner().concrete,
        }
    }
}

impl_inject_api!(
    PlanAPI,
    Plan,
    "xuehua.planner",
    (package, "package"),
    (group, "group"),
);

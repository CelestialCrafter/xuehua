use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use log::{debug, info, warn};
use mlua::{AnyUserData, ExternalResult, Lua};
use petgraph::{Direction::Outgoing, graph::NodeIndex, visit::EdgeRef};
use thiserror::Error;

use crate::{
    executor::{self, LuaCommand, LuaOutput},
    package::{DependencyType, Package},
    planner::Planner,
    store,
};

const MODULE_NAME: &str = "xuehua.builder";

#[derive(Error, Debug)]
pub enum Error {
    #[error("conflicting link point at {0}")]
    Conflict(PathBuf),
    #[error(transparent)]
    StoreError(#[from] store::Error),
    #[error(transparent)]
    ExecutorError(#[from] executor::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

/// Package build runner
///
/// The builder traverses thru a [`Planner`]'s, instructions, and builds out all of the contents needed to link a package, while using a [`Store`](store::Store) as a build cache.
pub struct Builder<'a, S: store::Store> {
    store: &'a mut S,
    planner: &'a Planner,
}

impl<'a, S: store::Store> Builder<'a, S> {
    pub fn new(store: &'a mut S, planner: &'a Planner) -> Self {
        Self { store, planner }
    }

    pub fn build<E: executor::Executor, F: FnMut() -> Result<E, executor::Error>>(
        &mut self,
        lua: &Lua,
        node: NodeIndex,
        mut make_executor: F,
    ) -> Result<HashSet<PathBuf>, Error> {
        let plan = self.planner.plan();
        let mut runtime = HashSet::new();

        let pkg_content = |store: &S, node: NodeIndex| {
            let pkg = store.package(&plan[node])?;
            let content = store.content(&pkg.artifact)?;

            Ok::<_, store::Error>(content)
        };

        let mut order: Vec<_> = plan.range(plan.get_position(node)..).collect();
        // for some reason, Acyclic::range returns in reverse topological order
        order.reverse();

        for node in order {
            debug!("resolving {node:?}");
            // add dependency outputs to their coresponding locations
            let mut buildtime = HashSet::new();
            for edge in plan.edges_directed(node, Outgoing) {
                let target = edge.target();
                let content = pkg_content(self.store, target)?;
                match edge.weight() {
                    DependencyType::Buildtime => {
                        debug!("adding {target:?} to buildtime closure");
                        buildtime.insert(content);
                    }
                    DependencyType::Runtime => {
                        debug!("adding {target:?} to runtime closure");
                        runtime.insert(content);
                    }
                }
            }

            let content = match pkg_content(self.store, node) {
                Ok(content) => {
                    info!("using cached package {node:?}");
                    content
                }
                // cache miss, build package
                Err(store::Error::PackageNotFound(_)) => {
                    info!("building package {node:?}");
                    let package = &plan[node];
                    let dependencies = runtime.union(&buildtime).map(|v| v.as_path()).collect();
                    let content = self.build_one(lua, package, dependencies, &mut make_executor)?;
                    let artifact = self.store.register_artifact(&content)?;
                    self.store.register_package(package, &artifact)?;

                    content
                }
                Err(err) => return Err(err.into()),
            };

            if node == node {
                runtime.insert(content);
            }
        }

        Ok(runtime)
    }

    fn build_one<B: executor::Executor, F: FnMut() -> Result<B, executor::Error>>(
        &self,
        lua: &Lua,
        package: &Package,
        dependencies: Vec<&Path>,
        mut make_builder: F,
    ) -> Result<PathBuf, Error> {
        let mut builder = (make_builder)()?;
        builder.init(dependencies)?;

        lua.scope(|scope| {
            let module = lua.create_table()?;
            module.set(
                "run",
                scope.create_function_mut(|_lua, userdata: AnyUserData| {
                    let command = &userdata.borrow::<LuaCommand>()?.0;
                    let output = builder.run(&command).into_lua_err()?;
                    LuaOutput::try_from(output).into_lua_err()
                })?,
            )?;
            module.set("Command", lua.create_proxy::<LuaCommand>()?)?;

            lua.register_module(MODULE_NAME, module)?;
            scope.add_destructor(|| {
                if let Err(err) = lua.unload_module(MODULE_NAME) {
                    warn!(error:? = err; "could not unregister {MODULE_NAME}");
                }
            });

            package.build.call::<()>(())
        })?;

        Ok(builder.output().to_path_buf())
    }
}

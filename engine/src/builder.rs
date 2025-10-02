use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

use log::{debug, info};
use mlua::{ExternalResult, Lua, Table};
use petgraph::{Direction::Outgoing, graph::NodeIndex, visit::EdgeRef};
use tempfile::TempDir;
use thiserror::Error;

use crate::{
    executor,
    package::{LinkTime, Package},
    planner::Plan,
    store,
};

const MODULE_NAME: &str = "xuehua.executor";

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    StoreError(#[from] store::Error),
    #[error(transparent)]
    ExecutorError(#[from] executor::Error),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub struct BuilderOptions {
    pub build_dir: PathBuf,
}

/// Package build runner
///
/// The builder traverses thru a [`Planner`]'s, instructions, and builds out all of the contents needed to link a package, while using a [`Store`](store::Store) as a build cache.
pub struct Builder<'a, S> {
    options: BuilderOptions,
    store: &'a mut S,
    plan: &'a Plan,
    executor_manager: executor::Manager,
}

impl<'a, S: store::Store> Builder<'a, S> {
    pub fn new(
        store: &'a mut S,
        plan: &'a Plan,
        executor_manager: executor::Manager,
        options: BuilderOptions,
    ) -> Self {
        Self {
            options,
            store,
            plan,
            executor_manager,
        }
    }

    pub fn build(&mut self, lua: &Lua, root: NodeIndex) -> Result<HashSet<PathBuf>, Error> {
        let pkg_content = |store: &S, node: NodeIndex| {
            let pkg = store.package(&self.plan[node].id)?;
            let content = store.content(&pkg.artifact)?;

            Ok::<_, store::Error>(content)
        };

        let mut order: Vec<_> = self.plan.range(self.plan.get_position(root)..).collect();
        // for some reason, Acyclic::range returns in reverse topological order
        order.reverse();

        let module = lua.create_table()?;
        lua.register_module(MODULE_NAME, &module)?;

        let mut runtime = HashSet::new();
        for node in order {
            debug!("resolving {node:?}");
            // add dependency outputs to their corresponding locations
            let mut buildtime = HashSet::new();
            for edge in self.plan.edges_directed(node, Outgoing) {
                let target = edge.target();
                let content = pkg_content(self.store, target)?;

                match edge.weight() {
                    LinkTime::Buildtime => {
                        debug!("adding {target:?} to buildtime closure");
                        buildtime.insert(content);
                    }
                    LinkTime::Runtime => {
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
                // cache miss, build and register package
                Err(store::Error::PackageNotFound(_)) => {
                    info!("building package {node:?}");
                    let package = &self.plan[node];
                    let dependencies = runtime.union(&buildtime).map(|v| v.as_path()).collect();
                    let environment = self.build_one(lua, &module, package, dependencies)?;

                    let artifact = self
                        .store
                        .register_artifact(&environment.path().join("output"))?;
                    self.store.register_package(package, &artifact)?;
                    self.store.content(&artifact)?
                }
                Err(err) => return Err(err.into()),
            };

            if node == root {
                debug!("adding {node:?} to runtime closure");
                runtime.insert(content);
            }
        }

        lua.unload_module(MODULE_NAME)?;
        Ok(runtime)
    }

    fn build_one(
        &self,
        lua: &Lua,
        module: &Table,
        package: &Package,
        dependencies: Vec<&Path>,
    ) -> Result<TempDir, Error> {
        let environment = tempfile::tempdir_in(&self.options.build_dir)?;

        lua.scope(|scope| {
            for name in self.executor_manager.registered() {
                module.set(
                    name,
                    scope.create_function(|lua, ()| {
                        let executor = self
                            .executor_manager
                            .create(name, environment.path().to_path_buf())
                            .into_lua_err()?;
                        Ok(lua.create_userdata(executor::LuaExecutor(executor)))
                    })?,
                )?;
            }

            package.build()
        })?;

        Ok(environment)
    }
}

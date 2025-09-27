use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use log::{debug, info, warn};
use mlua::{AnyUserData, ExternalResult, Lua};
use petgraph::{Direction::Outgoing, graph::NodeIndex, visit::EdgeRef};
use thiserror::Error;

use crate::{
    modules::{
        builder::{Builder, BuilderError, LuaCommand, LuaOutput},
        planner::Planner,
        store::{Store, StoreError},
    },
    package::{DependencyType, Package},
};

const MODULE_NAME: &str = "xuehua.resolver";

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("conflicting link point at {0}")]
    Conflict(PathBuf),
    #[error(transparent)]
    StoreError(#[from] StoreError),
    #[error(transparent)]
    BuilderError(#[from] BuilderError),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

pub struct Resolver<'a, S: Store> {
    store: &'a mut S,
    planner: &'a Planner,
}

impl<'a, S: Store> Resolver<'a, S> {
    pub fn new(store: &'a mut S, planner: &'a Planner) -> Self {
        Self { store, planner }
    }

    pub fn resolve<B: Builder, F: FnMut() -> Result<B, BuilderError>>(
        &mut self,
        lua: &Lua,
        root: NodeIndex,
        mut make_builder: F,
    ) -> Result<HashSet<PathBuf>, ResolverError> {
        let plan = self.planner.plan();
        let mut runtime = HashSet::new();

        let pkg_content = |store: &S, node: NodeIndex| {
            let pkg = store.package(&plan[node])?;
            let content = store.content(&pkg.artifact)?;

            Ok::<_, StoreError>(content)
        };

        let mut order: Vec<_> = plan.range(plan.get_position(root)..).collect();
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
                    debug!("using cached package {node:?}");
                    content
                }
                // cache miss, build package
                Err(StoreError::PackageNotFound(_)) => {
                    info!("building package {node:?}");
                    let package = &plan[node];
                    let dependencies = runtime.union(&buildtime).map(|v| v.as_path()).collect();
                    let content = self.build(lua, package, dependencies, &mut make_builder)?;
                    let artifact = self.store.register_artifact(&content)?;
                    self.store.register_package(package, &artifact)?;

                    content
                }
                Err(err) => return Err(err.into()),
            };

            if node == root {
                runtime.insert(content);
            }
        }

        Ok(runtime)
    }

    fn build<B: Builder, F: FnMut() -> Result<B, BuilderError>>(
        &self,
        lua: &Lua,
        package: &Package,
        dependencies: Vec<&Path>,
        mut make_builder: F,
    ) -> Result<PathBuf, ResolverError> {
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

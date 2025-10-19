use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

use mlua::{ExternalResult, UserDataMethods};
use petgraph::{
    Direction,
    graph::NodeIndex,
    visit::{DfsPostOrder, Visitable},
};
use tempfile::TempDir;
use thiserror::Error;

use crate::{
    executor,
    package::{LinkTime, Package},
    planner::{Plan, Planner},
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

#[derive(Debug, Clone, Copy)]
pub struct EnvironmentIndex(usize);

#[derive(Debug, Clone)]
pub struct BuilderOptions {
    pub build_dir: PathBuf,
}

/// Package build runner
///
/// The builder traverses through a [`Planner`]'s instructions and builds all of the environments needed to link the target package
pub struct Builder<'a> {
    planner: &'a Planner<'a>,
    executors: &'a executor::Manager,
    visitor: DfsPostOrder<NodeIndex, <Plan as Visitable>::Map>,
    environments: Vec<TempDir>,
    runtime: HashSet<usize>,
    buildtime: HashSet<usize>,
    options: BuilderOptions,
}

impl<'a> Iterator for Builder<'a> {
    type Item = Result<(&'a Package, EnvironmentIndex), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let plan = self.planner.plan();
        let node = self.visitor.next(plan)?;
        let pkg = &plan[node];

        let dependencies = self.runtime.union(&self.buildtime).copied().collect();
        match self.build_impl(pkg, dependencies) {
            // insert the environment as a dependency for all parents
            Ok(environment) => {
                let env_idx = self.environments.len();
                self.environments.push(environment);

                // all descendant runtime packages need to be linked alongside the target, so the they're being persisted
                // the only buildtime packages needed are direct descendants, so they need to be cleared every build
                self.buildtime.clear();
                for edge in plan.edges_directed(node, Direction::Incoming) {
                    match edge.weight() {
                        LinkTime::Runtime => &mut self.runtime,
                        LinkTime::Buildtime => &mut self.buildtime,
                    }
                    .insert(env_idx);
                }

                Some(Ok((pkg, EnvironmentIndex(env_idx))))
            }
            Err(err) => Some(Err(err)),
        }
    }
}

impl<'a> Builder<'a> {
    pub fn new(
        target: NodeIndex,
        planner: &'a Planner,
        executors: &'a executor::Manager,
        options: BuilderOptions,
    ) -> Self {
        Self {
            options,
            visitor: DfsPostOrder::new(&planner.plan(), target),
            planner,
            executors,
            environments: Vec::new(),
            runtime: HashSet::new(),
            buildtime: HashSet::new(),
        }
    }

    // NOTE: `EnvironmentIndex` is not publically constructable, so directly indexing `self.environments` is fine
    pub fn environment(&self, index: EnvironmentIndex) -> &Path {
        self.environments[index.0].path()
    }

    fn build_impl(&self, pkg: &Package, dependencies: Vec<usize>) -> Result<TempDir, Error> {
        // setup
        let lua = self.planner.lua();

        // TODO: link dependencies
        let environment = TempDir::new_in(&self.options.build_dir)?;
        fs::create_dir(environment.path().join("output"))?;

        let executors = self
            .executors
            .registered()
            .into_iter()
            .map(|name| {
                self.executors
                    .new(name, environment.path())
                    // registered() is guaranteed to only return valid names by Manager::register(), so .unwrap() is fine
                    .unwrap()
                    .map(|executor| (name, executor))
            })
            .collect::<Result<Vec<_>, executor::Error>>()?;

        // insert executors into lua and build the package
        let result = lua.scope(|scope| {
            let module = lua.create_table()?;
            lua.register_module(MODULE_NAME, &module)?;

            for (name, executor) in executors {
                module.set(
                    name,
                    scope.create_any_userdata(executor, |registry| {
                        registry.add_method("create", |lua, this, args| {
                            this.create(lua, args).into_lua_err()
                        });

                        registry.add_async_method_mut("dispatch", async |lua, mut this, args| {
                            this.dispatch(lua, args).await.into_lua_err()
                        });
                    })?,
                )?;
            }

            pkg.build()
        });

        lua.unload_module(MODULE_NAME)?;

        match result {
            Ok(_) => Ok(environment),
            Err(err) => Err(err.into()),
        }
    }
}

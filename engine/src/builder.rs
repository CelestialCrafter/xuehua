use std::{
    collections::{HashMap, HashSet},
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
    package::{self, LinkTime, Package},
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
    cache: HashMap<package::id::Id, PathBuf>,
    visitor: DfsPostOrder<NodeIndex, <Plan as Visitable>::Map>,
    outputs: Vec<PathBuf>,
    runtime: HashSet<usize>,
    buildtime: HashSet<usize>,

    planner: &'a Planner<'a>,
    executors: &'a executor::Manager,

    options: BuilderOptions,
}

impl<'a> Iterator for Builder<'a> {
    type Item = Result<(&'a Package, EnvironmentIndex), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let plan = self.planner.plan();
        let node = self.visitor.next(plan)?;
        let pkg = &plan[node];

        let dependencies = self.runtime.union(&self.buildtime).copied().collect();
        let output = match self.cache.remove(&pkg.id) {
            Some(content) => Ok(content),
            None => self.build_impl(pkg, dependencies),
        };

        Some(output.map(|output| {
            let out_idx = self.outputs.len();
            self.outputs.push(output);

            // all descendant runtime packages need to be linked alongside the target, so the they're being persisted
            // the only buildtime packages needed are direct descendants, so they need to be cleared every build
            self.buildtime.clear();
            for edge in plan.edges_directed(node, Direction::Incoming) {
                match edge.weight() {
                    LinkTime::Runtime => &mut self.runtime,
                    LinkTime::Buildtime => &mut self.buildtime,
                }
                .insert(out_idx);
            }

            (pkg, EnvironmentIndex(out_idx))
        }))
    }
}

impl<'a> Builder<'a> {
    pub fn new(
        target: NodeIndex,
        planner: &'a Planner,
        executors: &'a executor::Manager,
        cache: HashMap<package::id::Id, PathBuf>,
        options: BuilderOptions,
    ) -> Self {
        Self {
            options,
            visitor: DfsPostOrder::new(&planner.plan(), target),
            planner,
            executors,
            cache,
            outputs: Vec::default(),
            runtime: HashSet::default(),
            buildtime: HashSet::default(),
        }
    }

    // NOTE: `EnvironmentIndex` is not publically constructable, so directly indexing `self.environments` is fine
    pub fn output(&self, index: EnvironmentIndex) -> &Path {
        &self.outputs[index.0]
    }

    fn build_impl(&mut self, pkg: &Package, dependencies: Vec<usize>) -> Result<PathBuf, Error> {
        // setup
        let lua = self.planner.lua();

        // TODO: link dependencies
        let environment = TempDir::new_in(&self.options.build_dir)?.keep();
        let output = environment.join("output");
        fs::create_dir(&output)?;

        let executors = self
            .executors
            .registered()
            .into_iter()
            .map(|name| {
                self.executors
                    .new(name, &environment)
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

                        registry.add_method_mut("dispatch", |lua, this, args| {
                            this.dispatch(lua, args).into_lua_err()
                        });
                    })?,
                )?;
            }

            pkg.build()
        });

        lua.unload_module(MODULE_NAME)?;

        match result {
            Ok(_) => Ok(output),
            Err(err) => Err(err.into()),
        }
    }
}

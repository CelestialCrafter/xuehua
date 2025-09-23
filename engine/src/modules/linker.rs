use std::{collections::HashMap, path::PathBuf};

use log::warn;
use mlua::{AnyUserData, ExternalResult, Lua};
use petgraph::{Direction::Outgoing, graph::NodeIndex, visit::EdgeRef};
use thiserror::Error;

use crate::{
    modules::{
        builder::{Builder, BuilderError, LuaCommand, LuaOutput},
        planner::Plan,
    },
    package::{DependencyType, Package},
};

const MODULE_NAME: &str = "xuehua.linker";

type Source = PathBuf;
type Destination = PathBuf;
type Output = HashMap<Destination, Source>;

#[derive(Error, Debug)]
pub enum LinkerError {
    #[error("conflicting link point at {0}")]
    Conflict(PathBuf),
    #[error(transparent)]
    BuilderError(#[from] BuilderError),
    #[error(transparent)]
    LuaError(#[from] mlua::Error),
}

type MakeBuilder<B> = fn() -> Result<B, BuilderError>;

pub struct Linker<B: Builder> {
    make_builder: MakeBuilder<B>,
}

fn get_store(node: NodeIndex, store: &HashMap<NodeIndex, Output>) -> Output {
    store
        .get(&node)
        .expect("dependency should already be built")
        // real store should return owned output
        .clone()
}

impl<B: Builder> Linker<B> {
    pub fn new(make_builder: MakeBuilder<B>) -> Self {
        Self { make_builder }
    }

    pub fn link(
        &self,
        lua: &Lua,
        plan: &Plan,
        root: NodeIndex,
    ) -> Result<Vec<Output>, LinkerError> {
        let mut store: HashMap<NodeIndex, Output> = HashMap::new();
        let mut runtime = Vec::new();

        let mut order: Vec<_> = plan.range(plan.get_position(root)..).collect();
        // for some reason, Acyclic::range returns in reverse topological order
        order.reverse();

        for node in order {
            // add dependency outputs to their coresponding locations
            let mut buildtime = Vec::new();
            for edge in plan.edges_directed(node, Outgoing) {
                let output = get_store(edge.target(), &store);
                match edge.weight() {
                    DependencyType::Buildtime => buildtime.push(output),
                    DependencyType::Runtime => runtime.push(output),
                }
            }

            let output = self.build(lua, &plan[node], &runtime, &buildtime)?;
            store.insert(node, output);
        }

        runtime.push(get_store(root, &store));
        Ok(runtime)
    }

    fn build(
        &self,
        lua: &Lua,
        package: &Package,
        _runtime: &[Output],
        _buildtime: &[Output],
    ) -> Result<Output, LinkerError> {
        let mut builder = (self.make_builder)()?;

        let mut output: Output = lua.scope(|scope| {
            let module = lua.create_table()?;
            module.set(
                "run",
                scope.create_function_mut(|_lua, userdata: AnyUserData| {
                    let command = &userdata.borrow::<LuaCommand>()?.0;
                    let output = builder.run(&command).into_lua_err()?;
                    LuaOutput::try_from(output).into_lua_err()
                })?,
            )?;

            lua.register_module(MODULE_NAME, module)?;
            scope.add_destructor(|| {
                if let Err(err) = lua.unload_module(MODULE_NAME) {
                    warn!(error:? = err; "could not unregister {MODULE_NAME}");
                }
            });

            package.build.call(())
        })?;
        output.insert("/package-id".into(), "/package-id".into());

        Ok(output)
    }
}

use crate::pkgsys::{PkgSysError, package::Package};
use eyre::eyre;
use mlua::{FromLua, LuaSerdeExt, Table};
use petgraph::{
    acyclic::Acyclic,
    data::Build,
    graph::{DiGraph, NodeIndex},
};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

type Graph<'a> = Acyclic<DiGraph<&'a PackageDependency, ()>>;

#[derive(Deserialize, Debug)]
pub enum DependencyType {
    Runtime,
    Comptime,
}

#[derive(Debug)]
pub struct PackageDependency {
    package: Package,
    pub dependency_type: DependencyType,
}

// TODO: revise this, the conversion to Table seems unneccesary
impl FromLua for PackageDependency {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value.clone(), lua)?;

        Ok(Self {
            package: Package::from_lua(value, lua)?,
            dependency_type: lua.from_value(table.get("type")?)?,
        })
    }
}

impl Deref for PackageDependency {
    type Target = Package;

    fn deref(&self) -> &Self::Target {
        &self.package
    }
}

impl DerefMut for PackageDependency {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.package
    }
}

pub struct DependencyResolver<'a> {
    graph: Graph<'a>,
    pub root: NodeIndex,
}

impl<'a> Deref for DependencyResolver<'a> {
    type Target = Graph<'a>;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl<'a> DependencyResolver<'a> {
    pub fn new(root: &'a PackageDependency) -> Self {
        let mut graph: Graph = Acyclic::new();
        let root = graph.add_node(root);

        Self { graph, root }
    }

    pub fn resolve(&mut self, from: NodeIndex) -> Result<(), PkgSysError> {
        let mut queue = VecDeque::from([from]);

        while let Some(parent) = queue.pop_front() {
            for child in &self.graph[parent].dependencies {
                let child = self.graph.add_node(&child);
                self.graph
                    .try_add_edge(parent, child, ())
                    .map_err(|_| PkgSysError::Cyclic(eyre!("cycle detected")))?;

                queue.push_back(child);
            }
        }

        Ok(())
    }
}

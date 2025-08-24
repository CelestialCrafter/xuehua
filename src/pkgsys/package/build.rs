use eyre::{Context, eyre};
use mlua::Value::Nil;
use petgraph::{
    Direction::Outgoing,
    acyclic::Acyclic,
    graph::{DiGraph, NodeIndex},
};
use std::{
    collections::HashMap,
    iter::once,
    ops::Deref,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

use crate::pkgsys::{
    PkgSysError,
    package::{
        Package,
        dependencies::{DependencyResolver, PackageDependency},
    },
};

pub type LinkPoints = HashMap<PathBuf, ()>;

enum NodeType {
    // TODO: figure out link points
    Evaluated(LinkPoints),
    Unevaluated,
}

type Node<'a> = (&'a PackageDependency, NodeType);

fn build(package: &Package, dependencies: Vec<LinkPoints>) -> Result<LinkPoints, PkgSysError> {
    // TODO: add package name
    eprintln!("building package");
    let points = LinkPoints::default();

    let env = package
        .build
        .environment()
        .expect("build() should not be a c/rust function");
    // TODO: impl builder api
    env.set("builder", Nil)?;
    package.build.set_environment(env)?;

    &package.build.call::<()>(())?;

    Ok(points)
}

type Graph<'a> = Acyclic<DiGraph<Node<'a>, ()>>;

pub struct LinkResolver<'a> {
    graph: Graph<'a>,
}

impl<'a> Deref for LinkResolver<'a> {
    type Target = Graph<'a>;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl<'a> LinkResolver<'a> {
    pub fn new(dependencies: &'a DependencyResolver<'a>) -> Self {
        Self {
            graph: Acyclic::try_from_graph(
                dependencies.map(|_, w| (*w, NodeType::Unevaluated), |_, w| *w),
            )
            .expect("DependencyResolver should be acyclic"),
        }
    }

    pub fn resolve(&mut self, from: NodeIndex) -> Result<LinkPoints, PkgSysError> {
        // eval all nodes in topological order
        self.graph
            .range(self.graph.get_position(from)..)
            .try_fold(|current, node| {
                let (pkg, node_type) = &mut self.graph[node];
                // TODO: turn dependencies into their link points and filter to only build time,
                // and add runtime dependencies to a graph shipped with the final package
                *node_type = NodeType::Evaluated(build(pkg, pkg.dependencies)?);
            });

        match self.graph[from].1 {
            NodeType::Evaluated(points) => points,
            NodeType::Unevaluated => return Err(),
        }
    }
}

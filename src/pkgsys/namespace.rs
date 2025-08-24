use crate::pkgsys::PkgSysError;
use crate::pkgsys::package::Package;
use eyre::eyre;
use mlua::{FromLua, Function, Lua, LuaSerdeExt, StdLib};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct NamespaceMetadata;

pub struct Namespace {
    pub name: String,
    pub dependencies: Vec<PathBuf>,
    pub metadata: NamespaceMetadata,
    pub build: Function,
}

impl FromLua for Namespace {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        let table = mlua::Table::from_lua(value, lua)?;

        Ok(Self {
            name: table.get("name")?,
            dependencies: table.get("dependencies")?,
            metadata: lua.from_value(table.get("metadata")?)?,
            build: table.get("build")?,
        })
    }
}

enum Node {
    Unevaluated(PathBuf),
    Evaluated(Namespace),
}

pub struct NamespaceResolver {
    graph: DiGraph<Node, ()>,
    pub root: NodeIndex,
}

impl Deref for NamespaceResolver {
    type Target = DiGraph<Node, ()>;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl NamespaceResolver {
    pub fn new(root: PathBuf) -> Self {
        let mut graph = DiGraph::new();
        let root = graph.add_node(Node::Unevaluated(root));

        Self { graph, root }
    }

    fn eval(&mut self, index: NodeIndex) -> Result<NodeIndex, PkgSysError> {
        let path = match &self.graph[index] {
            Node::Unevaluated(path) => path,
            Node::Evaluated(_) => return Ok(index),
        };

        eprintln!("evaluating {path:?}");
        let chunk = fs::read(path).map_err(|err| PkgSysError::Other(err.into()))?;

        let lua = Lua::new();
        // TODO: make sure there's no direct access to the filesystem
        lua.load_std_libs(StdLib::ALL_SAFE)?;

        let mut namespace = Namespace::default();

        let globals = lua.globals();
        lua.scope(|scope| {
            let add_package = scope.create_function_mut(|lua, (name, pkg)| {
                if namespace.packages.contains_key(&name) {
                    let conflict =
                        PkgSysError::Conflict(eyre!("package {name} is already registered"));
                    Err(conflict.into())
                } else {
                    namespace.packages.insert(name, lua.from_value(pkg)?);
                    Ok(())
                }
            })?;

            let add_namespace = scope.create_function_mut(|_, (name, path)| {
                if namespace.children.contains_key(&name) {
                    let conflict =
                        PkgSysError::Conflict(eyre!("namespace {name} is already registered"));
                    Err(conflict.into())
                } else {
                    let child = self.graph.add_node(Node::Unevaluated(path));
                    self.graph.add_edge(index, child, ());
                    namespace.children.insert(name, child);
                    Ok(())
                }
            })?;

            globals.set("package", add_package)?;
            globals.set("namespace", add_namespace)?;

            lua.load(chunk).exec()
        })
        .map_err(PkgSysError::from)?;

        *(&mut self.graph[index]) = Node::Evaluated(namespace);
        Ok(index)
    }

    pub fn resolve<'a>(
        &'a mut self,
        from: NodeIndex,
        path: &Vec<String>,
    ) -> Result<NodeIndex, PkgSysError> {
        path.iter().try_fold(self.eval(from)?, |previous, current| {
            match &self.graph[previous] {
                Node::Evaluated(ns) => {
                    let not_found = PkgSysError::NotFound(eyre!("namespace {} not found", current));
                    self.eval(ns.children.get(current).copied().ok_or(not_found)?)
                }
                _ => unreachable!("namespace should be evaluated"),
            }
        })
    }
}

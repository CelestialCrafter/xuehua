use std::{
    collections::{HashMap, hash_map::Entry},
    fs,
    path::{Path, PathBuf},
};

use crate::evaluator::EvaluationError;
use crate::package::{Id, Package};
use eyre::{Report, eyre};
use mlua::{Lua, LuaSerdeExt, StdLib, Value::Nil};
use petgraph::{acyclic::Acyclic, graph::DiGraph};

fn insert_or_err<K, V>(
    entry: Entry<'_, K, V>,
    value: V,
    err: Report,
) -> Result<mlua::Value, mlua::Error> {
    match entry {
        Entry::Occupied(_) => Err(EvaluationError::Conflict(err).into()),
        Entry::Vacant(vacant) => {
            vacant.insert(value);
            Ok(Nil)
        }
    }
}

type PackageGraph = Acyclic<DiGraph<Package, ()>>;
type NamespaceContainer<T> = HashMap<Vec<String>, T>;
pub type Namespace = HashMap<String, Package>;

pub struct Resolver<'a> {
    pub root: &'a Path,
    pub ns_lookup: NamespaceContainer<PathBuf>,
    pub ns_packages: NamespaceContainer<Namespace>,
    pub packages: PackageGraph,
}

impl<'a> Resolver<'a> {
    pub fn new(root: &'a Path) -> Resolver<'a> {
        Resolver {
            root,
            ns_lookup: HashMap::new(),
            ns_packages: HashMap::new(),
            packages: Acyclic::new(),
        }
    }

    fn eval_ns(&mut self, breadcrumbs: Vec<String>, path: PathBuf) -> Result<(), EvaluationError> {
        let path: &Path = &path;
        eprintln!("evaluating {path:?}");
        let lua = Lua::new();

        // TODO: make sure there's no direct access to the filesystem
        lua.load_std_libs(StdLib::ALL_SAFE)?;

        let namespace = self.ns_packages.entry(breadcrumbs).or_default();
        lua.scope(|scope| {
            let globals = lua.globals();

            let add_package = scope.create_function_mut(|lua, (name, pkg_val)| {
                let report = eyre!("package {name} is already registered");
                insert_or_err(namespace.entry(name), lua.from_value(pkg_val)?, report)
            })?;

            let add_namespace =
                scope.create_function_mut(|_, (name, path): (String, PathBuf)| {
                    let report = eyre!("namespace {name} is already registered");
                    let key = [breadcrumbs.as_slice(), &[name]].concat();
                    insert_or_err(self.ns_lookup.entry(key), path, report)
                })?;

            globals.set("package", add_package)?;
            globals.set("namespace", add_namespace)?;

            lua.load(fs::read(path)?).exec()?;

            Ok(())
        })
        .map_err(EvaluationError::from)
    }

    pub fn find(&mut self, desired: &Id) -> Result<&Package, EvaluationError> {
        let final_namespace = desired.namespaces.iter().try_fold(
            self.eval_ns(self.root.to_path_buf())?,
            |parent_idx, ns_name| -> Result<_, EvaluationError> {
                let parent_ns = &self.namespaces[parent_idx];

                let child_path = parent_ns
                    .namespaces
                    .get(ns_name)
                    .ok_or(EvaluationError::NotFound(eyre!(
                        "namespace '{}' not found",
                        ns_name
                    )))?
                    .clone();

                let child_idx = self.cached_eval_ns(child_path)?;
                self.namespaces.update_edge(parent_idx, child_idx, ());
                Ok(child_idx)
            },
        )?;

        (&self.namespaces[final_namespace])
            .packages
            .get(&desired.package)
            .ok_or(EvaluationError::NotFound(eyre!(
                "package '{}' not found",
                desired.package
            )))
    }
}

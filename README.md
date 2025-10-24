# xuehua

## Abstract

A package manager and (eventually) distro inspired by nixpkgs/nixos.

## Modules
The parts that as a whole make up xuehua.

**Planner -**
Evaluates a xuehua project to generate a package dependency graph which the builder then uses.

**Builder -**
Systematically creates and manages packages' and their dependancies' environments based on the planner's graph.

**Executor -**
Executed by the builder to run sandboxed system actions in the package's custom environment (generally used to build a program, but is not limited to do so).

**Store -**
Read-only (only writable by xuehua itself) cache containing packages that have been linked to the current system.

**Linker -**
Moves a package into the scope of the current system, or removes it.

## Concepts

**namespace**
a repository or group name for packages (ex. xuehua, alpine, debian, celestial)
`local` is a special name that means "the current namespace"

**package**
a namespace, name, and version in the format of `[path to folder containing xuehua.toml/]<namespace>:<name>[@version]`

**store**
a read-only (writable by `xh`) cache containing packages that have been linked/are linked

## commands

`xh shell <package>`:
sets up a sandboxed environment for a package,
and drops the user into it

`xh link <package>`:
sets up symlinks for a package in the running system

`xh unlink <package>`:
removes symlinks for a pcakage in the running system

`xh gc`:
removes unlinked packages from the store

## package api

**dependencies**

- `addBuild(pkg)` → build-time deps
- `addRuntime(pkg)` → runtime deps (could alias `addBuild`)
- `forceResolution(pkg, repo)` → local conflict resolution

**fetch**

- `network(url) -> store path`: fetch remote
- `local(path) -> store path`: fetch local

**unpack**

- `auto(path) -> path`: detect type & unpack
- `git(url) -> path`: clone
- `tar/xz/gzip/zstd(path) -> path`: unpack respective format

**commands**

- `shell(package, file, args) -> stdio handle`: run commands

bubblewrap sandbox by default (can be disabled from xuehua.toml)

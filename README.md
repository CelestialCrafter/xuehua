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
Makes a package be inaccessible/accessible to the current system (through one of few ways).

## Concepts

**Namespace -**
A repository or group name for packages (ex. xuehua, alpine, debian, celestial). There is a special group name, called `local` that means the "currently selected namespace".

**Package -**
A namespace, name, and version in the format of `[path to folder containing xuehua.toml/]<namespace>:<name>[@version]`.

## Commands

`xh shell <package>` -
Sets up a sandboxed environment for a package, and drops the user into it.

`xh link <package>` -
Make a package accessible to the current system.

`xh unlink <package>` -
Make a package inaccessible to the current system.

`xh gc` -
Removes inaccessible packages from the store.

## Packaging API

**Dependencies**

- `addBuild(pkg)` → Build-time deps
- `addRuntime(pkg)` → Runtime deps (could alias `addBuild`)
- `forceResolution(pkg, repo)` → Local conflict resolution

**Fetch**

- `network(url) → store path` → Fetch remote
- `local(path) → store path` → Fetch local

**Unpack**

- `auto(path) → path` → Detect type & unpack
- `git(url) → path` → Clone
- `tar/xz/gzip/zstd(path) → path` → Unpack respective format

**Commands**

- `shell(package, file, args) → stdio handle` → Run commands

Has a bubblewrap sandbox by default (can be disabled from xuehua.toml).

# xuehua

## idea

package manager/distro inspired by nixpkgs/nixos

## concepts

**namespace**
a repository or group name for packages (ex. xuehua, alpine, debian, celestial)
`local` is a special name that means "the current namespace"

**package**
a namespace, name, and version in the format of `[path to folder containing xuehua.toml/]<namespace>:<name>[@version]`

**store**
a read-only (writable by `xh`) cache containing packages that have been linked/are linked

## commands

`xh link <package>`:
sets up symlinks for a package in the running system

`xh unlink <package>`:
removes symlinks for a pcakage in the running system

`xh gc`:
removes unlinked packages from the store

## api

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

# Xuehua

A build system framework and package manager inspired by [NixOS](https://nixos.org/).

## Features

- **Transparent:** A complete graph of package declarations and build steps are fully visible before you run or download anything.
- **Scriptable:** Systems are scripted with [Lua](https://www.lua.org/), which allows for declarative and dynamic systems.
- **Reproducible:** Package builds are fully isolated from eachother via [Bubblewrap](https://github.com/containers/bubblewrap), ensuring build artifacts are fully reproducible and don't have implicit dependencies.
- **Flexible:** If you exceed the capabilities of the Package Manager, the Engine API provides the flexibility to switch out the Stores, Backends, or Executors.

## Documentation

Xuehua documentation can be found in the [book](https://xuehua.celestial.moe), including instructions for [Installation](https://xuehua.celestial.moe/package-manager/installation.html) and [Getting Started](https://xuehua.celestial.moe/package-manager/getting-started.html)

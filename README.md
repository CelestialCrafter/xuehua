# Xuehua

A flexible build system framework and package manager for Unix-like systems.

## Features

- **Transparent:** A complete graph of package declarations and build steps are fully visible before you run or download anything.
- **Scriptable:** Systems are scripted with [Lua](https://www.lua.org/), which allows for declarative and dynamic systems.
- **Reproducible:** Package builds are fully isolated from eachother via [Bubblewrap](https://github.com/containers/bubblewrap), ensuring build artifacts are fully reproducible and don't have implicit dependencies.
- **Uses existing infrastructure:** By relying on existing open-source packaging infrastructure from existing distributions like [Alpine Linux](https://alpinx.org/) or [Arch Linux](https://archlinux.org/), Xuehua provide a better building and packaging experience, while providing you with reliable artifacts.
- **Flexible:** If you exceed the capabilities of the Package Manager, the Engine API provides the flexibility to switch out the Artifact Store, Planning Backend, Command Runner, and more.

## Documentation

Xuehua documentation can be found in the [book](https://xuehua.celestial.moe), including instructions for [Installation](https://xuehua.celestial.moe/package-manager/installation.html) and [Getting Started](https://xuehua.celestial.moe/package-manager/getting-started.html)

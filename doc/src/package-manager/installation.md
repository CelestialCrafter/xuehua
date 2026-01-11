# Installation

Xuehua currently requires a Linux environment with user namespaces enabled.

## Prerequisites

Before installing, ensure you have the following dependencies:

- **[Busybox](https://www.busybox.net/):** Required for the Bubblewrap Executor.
- **[Bubblewrap](https://github.com/containers/bubblewrap):** Required for the Bubblewrap Executor.
- \***[Lua 5.4](https://www.lua.org/):** Required for the Lua Backend.
- \***[SQLite](https://sqlite.org/):** Required for the Local Store.

<small>Dependencies marked with \* only need the libraries</small>

## Building from Source

To build from source, the following build dependencies are needed alongside the
runtime dependencies listed above:

- **[Rust](https://rust-lang.org/):** Required for Xuehua.
- **[Clang](https://clang.llvm.org/):** Required for SQLite and Lua FFI.

To install Xuehua, run:
```sh
cargo install --git https://github.com/CelestialCrafter/xuehua --locked cli
```

# Xuehua

## Abstract

A package manager, build system, and (eventually) linux distribution. Inspired by [NixOS](https://nixos.org/).

## Subsystems

The individual systems that make up Xuehua.

**Planner** - Evaluates a Xuehua project to generate a package dependency graph.

**Builder** - Uses the planner's graph to schedule and run package builds.

**Executor** - Used by the builder to run sandboxed system actions (shell commands, network requests, etc) in the package's build environment.

**Store** - An append-only build artifact repository.

**Linker** - Adds or removes a package from the current running system (via copy, symlink, bind mount, etc).

## Commands

`xh link [package]... [root] [--reverse]` - Uses the Linker to manage packages on the system

`xh inspect plan [path] [--format <dot|json>]` - Prints the plan for a project. DOT output can be further processed by [Graphviz](https://graphviz.org/).

`xh inspect package [package]... [--format <human|json>]` - Prints the definitions for the given packages

## Planner API (`xuehua.planner`)

**This module's functions are only accessable during the planning phase.**

Functions to manage the planner's package graph.

### `package(options)`

Declares a package

Example:
```lua
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

plan.package(utils.no_config {
  name = "my_pacakge",
  dependencies = {
    utils.runtime(my_dependency_package)
    utils.buildtime(my_other_dependency_pacakge)
  },
  metadata = {
    maintainers = { "maintenance@celestial.moe" },
    homepage = "https://celestial.moe",
    version = "1.0.0",
    license = utils.licenses.mpl2
  },
  build = function() end
})
```

### `configure(options)`

Changes a package's definition options via its defined parameters.

Example:
```lua
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")
local log = require("xuehua.logger")

local package = plan.package {
  name = "my-configurable-package"
  defaults = { my_option = true }
  configure = function(opts)
    local deps = { utils.buildtime(my_dependency_package) }
    if opts.my_option then
      table.insert(deps, utils.runtime(my_optional_package))
    end

    return {
      dependencies = deps,
      build = function()
        log.info("my_option is currently set to " .. tostring(opts.my_option))
      end
    }
  end
}

plan.configure {
  source = package,
  destination = "my-final-package",
  modify = function(prev)
    prev.my_option = true
    return prev
  end
}
```

### `namespace(name, func)`

Appends a namespace segment to all packages defined within the function.
Namespaces are a package grouping mechanism designed to both improve user clarity, and to prevent name conflicts.

Example:
```lua
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local my_pkg = plan.package {
  name = "my-package"
}

local my_pkg_2 = plan.namespace("other-pkgs", function()
  return plan.package {
    name = "my-package"
  }
end)

plan.package {
  name = "my-final-package"
  dependencies = {
    utils.runtime(my_pkg),
    utils.runtime(my_pkg_2)
  }
}
```

## Executor API (`xuehua.executor.*`)

**This module's functions are only accessable during the building phase.**

The Executor API contains one module per executor registered.
You can access specific executors by using `xuehua.executor.<name>` as the module name.

Executors are used by first `create`-ing executor-specific data, and then `dispatch`-ing the data to an executor.

Example with fetch executor:
```lua
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")
local log = require("xuehua.logger")

plan.package(utils.no_config {
  name = "my-package",
  build = function()
    local curl = require("xuehua.executor.curl")
    local response = curl
      :create({
        url = "https://celestial.moe"
      })
      :dispatch()

    log.info("request completed with status " .. tostring(response.status))
  end
})
```

## Logger API (`xuehua.logger`)

Logging functions to communicate from within lua.

### `info(message)`, `warn(message)`, `error(message)`, `debug(message)`, and `trace(message)`

Functions to log messages with differing severity.

Example:
```lua
local log = require("xuehua.logger")
log.info("hello world!")
log.warn("hello world!")
log.error("hello world!")
log.debug("hello world!")
log.trace("hello world!")
```

## Utils API (`xuehua.utils`)

Utility functions to make programming easier.

### `runtime(package)` and `buildtime(package)`

Transforms a package into a runtime or buildtime dependency.

Example:
```lua
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local my_pkg = plan.package {
  name = "my_package"
}

-- Expands to { type = "runtime", package = my_pkg }
utils.runtime(my_pkg)
```

### `no_config(definition)`

Wraps a package definition to not have configuration.

Example:
```lua
local utils = require("xuehua.utils")

-- Expands to {
--   name = "my_package",
--   defaults = {},
--   configure = function(_opts)
--     return { license = utils.licenses.mpl2 }
--   end
-- }
utils.no_config {
  name = "my_package",
  metadata = {
    license = utils.licenses.mpl2
  }
}
```

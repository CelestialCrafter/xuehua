# Getting Started

This section covers the basics of creating and building a project with the Xuehua CLI.

## Project Structure

A minimal project consists of a `main.lua` file which defines your packages and the build steps.

Here's the example we're going to use:
```lua
local planner = require("xuehua.planner")

-- Package definition
planner:package {
  name = "hello-xuehua",
  apply = function(opts)
    -- Metadata
    local metadata = {
      version = "1.0.0",
      license = "MIT"
      homepage = { "https://celestial.moe/" },
    }

    -- Build steps
    local requests = {}
    table.insert(requests, {
      executor = "bubblewrap@xuehua/executors",
      payload = {
        program = "/busybox",
        arguments = { "touch", "hello-xuehua" }
      }
    })

    return {
      metadata = metadata,
      requests = requests
    }
  end
}

## Building

Before we start building, let's see exactly what we're going to build.
Ensure you're in the same directory as `main.lua` and run:
```sh
xh package inspect hello-xuehua --format json | jq
```

Which should output something similar to:
```json
[
  {
    "name": "hello-xuehua",
    "requests": [
      {
        "executor": "bubblewrap@xuehua/executors",
        "payload": {
          "program": "/busybox",
          "arguments": [ "touch", "hello-xuehua" ]
        }
      }
    ],
    "metadata": {
      "version": "1.0.0",
      "license": "MIT",
      "homepage": [ "https://celestial.moe/" ]
    }
  }
]
```

Now that we know what we're building, let's build it!
```sh
xh package build hello-xuehua
```

TODO: show build output
TODO: show how to package a real app
TODO: show how to use xuehua's builtin packages

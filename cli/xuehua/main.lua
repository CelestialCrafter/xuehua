local utils = require("xuehua.utils")
local ns = planner.namespace

local build = function(name)
  return function()
    local runner = executors.runner

    do
      local command = runner.create("/busybox")
      command.arguments = { "mkdir", "-p", "/output/test" }
      runner:dispatch(command)
    end

    do
      local command = runner.create("/busybox");
      command.arguments = { "touch", "/output/test/from-" .. name }
      runner:dispatch(command)
    end
  end
end


local p2 = planner:package(utils.no_config {
  name = "p2",
  dependencies = {},
  metadata = {},
  build = build("p2")
})

local p3 = planner:package(utils.no_config {
  name = "p3",
  dependencies = { utils.runtime(p2) },
  metadata = {},
  build = build("p3")
})

local p3a = ns:scope("my-ns", function()
  local pkg = planner:package(utils.no_config {
    name = "p3",
    dependencies = { utils.runtime(p2) },
    metadata = {},
    build = build("p3")
  })
  return pkg
end)

planner:package(utils.no_config {
  name = "p1",
  dependencies = { utils.runtime(p3a), utils.buildtime(p3) },
  metadata = {},
  build = build("p1")
})

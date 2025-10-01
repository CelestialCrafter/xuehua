local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local build = function(id)
  return function()
    local manager = require("xuehua.executor")
    local runner = manager.runner()

    do
      local command = runner:create("/busybox");
      command.arguments = { "mkdir", "-p", "/output/wawa" }
      runner:dispatch(command)
    end

    do
      local command = runner:create("/busybox");
      command.arguments = { "touch", "/output/wawa/from-" .. id }
      runner:dispatch(command)
    end
  end
end


local p3 = plan.package(utils.no_config {
  id = "p3",
  dependencies = {},
  metadata = {},
  build = build("p3")
})

local p2 = plan.package(utils.no_config {
  id = "p2",
  dependencies = {},
  metadata = {},
  build = build("p2")
})

plan.package(utils.no_config {
  id = "p1",
  dependencies = { utils.runtime(p2), utils.buildtime(p3) },
  metadata = {},
  build = build("p1")
})

local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local build = function(id)
  return function()
    local builder = require("xuehua.builder")
    local command = builder.Command("/busybox");
    command.arguments = { "touch", "/output/wawa/from-" .. id }
    builder.run(command)
  end
end


local p3 = plan.package {
  id = "p3",
  dependencies = {},
  metadata = {},
  build = build("p3")
}

local p3a = plan.package {
  id = "p3",
  dependencies = {},
  metadata = {},
  build = build("p3")
}

local p2 = plan.package {
  id = "p2",
  dependencies = { utils.buildtime(p3a) },
  metadata = {},
  build = build("p2")
}

plan.package {
  id = "p1",
  dependencies = { utils.runtime(p2), utils.buildtime(p3) },
  metadata = {},
  build = build("p1")
}

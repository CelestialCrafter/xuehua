local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local build = function(id)
  return function()
    local resolver = require("xuehua.resolver")
    local command = resolver.Command("/busybox");
    command.arguments = { "touch", "/output/wawa/from-" .. id }
    resolver.run(command)
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

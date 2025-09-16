local log = require("xuehua.logger")
local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local build = function(id)
  return function()
    log.info(string.format("im building " .. id))
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
  dependencies = { utils.runtime(p3a) },
  metadata = {},
  build = build("p2")
}

plan.package {
  id = "p1",
  dependencies = { utils.runtime(p2), utils.runtime(p3) },
  metadata = {},
  build = build("p1")
}

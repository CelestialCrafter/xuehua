local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

local package_2 = plan.package {
  id = "package-2",
  dependencies = {},
  metadata = {},
  build = function() end
}

plan.package {
  id = "package-1",
  dependencies = { utils.runtime(package_2) },
  metadata = {},
  build = function() end
}

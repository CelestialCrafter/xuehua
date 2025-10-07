local plan = require("xuehua.planner")
local utils = require("xuehua.utils")

plan.package(utils.no_config {
  id = "package-3",
  dependencies = {},
  metadata = {},
  build = function() end
})

plan.package(utils.no_config {
  id = "package-3",
  dependencies = {},
  metadata = {},
  build = function() end
})

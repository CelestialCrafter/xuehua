local plan = require("xuehua.planner")
local utils = require("xuehua.utils")
local log = require("xuehua.logger")

local package_3 = plan.package(utils.no_config {
  id = "package-3",
  dependencies = {},
  metadata = {},
  build = function() end
})


local package_2 = plan.package {
  id = "package-2",
  defaults = { enable_thing = true },
  configure = function(options)
    local partial = {
      dependencies = {},
      metadata = {},
      build = function() end
    }

    if options.enable_thing then
      table.insert(partial.dependencies, utils.runtime(package_3))
    end

    return partial
  end
}

local package_2_a = plan.configure {
  source = package_2,
  destination = "package-2-a",
  modify = function(prev)
    prev.enable_thing = false
    return prev
  end
}

plan.package(utils.no_config {
  id = "package-1",
  dependencies = { utils.runtime(package_2), utils.buildtime(package_2_a) },
  metadata = {},
  build = function() end
})

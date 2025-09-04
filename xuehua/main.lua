-- local log = require("xuehua.logger")
local plan = require("xuehua.planner")
-- local exec = require("xuehua.executor")
-- local utils = require("xuehua.utils")

local build = function(id)
  return function()
    -- log.info(string.format("im building " .. id))

    -- local ls = exec.run("ls", { "/" })
    -- log.info(string.format("ls stdout: " .. ls.stdout))

    -- exec.run("sh", { "-c", string.format("echo 'hii! <3 (from %s)' > xuehua-test", id) })
    -- exec.link({ source = "xuehua-test", destination = "/xuehua-test-" .. id })
  end
end

-- plan.template {
--   id = "p0",
--   schema = { option_1 = true },
--   apply = function(destination, inputs)
--     -- log.info("im being configured with " .. inputs)
--     return {
--       dependencies = {},
--       metadata = {},
--       build = build(destination),
--     }
--   end
-- }

-- plan.profile {
--   source = "p0",
--   destination = "p0a",
--   inputs = {
--     option_1 = false
--   }
-- }

plan.group("g1", function(g1)
  -- log.info("im entering group " .. g1)
  plan.package {
    id = "p1",
    dependencies = {  },
    metadata = {},
    build = build("p1")
  }

  plan.package {
    id = "p3",
    dependencies = {},
    metadata = {},
    build = build("p3")
  }

  local g2 = plan.group("g2", function(g2)
    -- log.info(string.format("im entering group " .. g2))
    local p2 = plan.package {
      id = "p2",
      dependencies = {  },
      metadata = {},
      build = build("p2")
    }

    -- log.info("absolute reference to p2: " .. p2)
  end)

  -- log.info("absolute reference to g2: " .. g2)
end)

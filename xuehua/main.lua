local planner = require("xuehua.planner")
local ns = planner.namespace

ns:scope("alpine", function()
  local minirootfs = planner:package {
    name = "minirootfs",
    defaults = { release = "3.22", version = "3.22.2", arch = "aarch64" },
    apply = function(opts)
      local requests = {}

      local download = {
        url = string.format("https://dl-cdn.alpinelinux.org/v%s/releases/%s/alpine-minirootfs-%s-%s.tar.gz",
          opts.release, opts.arch, opts.version, opts.arch),
        method = "GET",
        path = "minirootfs.tar.gz"
      }

      table.insert(requests, {
        executor = "http@xuehua/executors",
        payload = download
      })

      local unpack = {
        program = "/busybox",
        arguments = { "sh", "-c", "/busybox tar -xf minirootfs.tar.gz -C output" }
      }

      table.insert(requests, {
        executor = "bubblewrap@xuehua/executors",
        payload = unpack
      })

      return {
        metadata = {},
        dependencies = {},
        requests = requests
      }
    end
  }

  return {
    minirootfs = minirootfs
  }
end)

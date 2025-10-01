local utils = require("xuehua.utils")
local ns = planner.namespace

ns:scope("alpine", function()
  local minirootfs = planner:package {
    name = "minirootfs",
    defaults = { release = "3.22", version = "3.22.2", arch = "aarch64" },
    dependencies = {},
    configure = function(opts)
      return {
        metadata = {},
        build = function()
          executors.http:dispatch(executors.http.create({
            url = string.format("https://dl-cdn.alpinelinux.org/v%s/releases/%s/alpine-minirootfs-%s-%s.tar.gz",
              opts.release, opts.arch, opts.version, opts.arch),
            method = "GET",
            path = "minirootfs.tar.gz"
          }))

          local unpack = [[
            /busybox mkdir output
            /busybox tar -xf minirootfs.tar.gz -C output
          ]]

          local shell = executors.bubblewrap.create("/busybox")
          shell.arguments = { "sh", "-c", unpack}
          executors.bubblewrap:dispatch(shell)
        end
      }
    end
  }

  return {
    minirootfs = minirootfs
  }
end)

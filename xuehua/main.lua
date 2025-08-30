package {
  name = "main",
  dependencies = { xuehua.utils.buildtime("xuehua/1.lua") },
  metadata = {},
  build = function()
    io.popen("echo hii! <3 > xuehua-test")
    xuehua.exec.link_point({ dest = "/etc/xuehua-test", src = "xuehua-test" })
  end
}

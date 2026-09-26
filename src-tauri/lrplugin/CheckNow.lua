local LrDialogs = import "LrDialogs"
local LrTasks = import "LrTasks"
local Service = require "Service"

LrTasks.startAsyncTask(function()
  local n = Service.tick()
  if n == 0 then
    LrDialogs.message("Safelight", "No photos waiting. Use Send in Safelight to hand some over.", "info")
  end
end)

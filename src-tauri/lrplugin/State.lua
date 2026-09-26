-- Shared by Init/Service/Shutdown: `require` hands every script the same table.
local LrFileUtils = import "LrFileUtils"
local LrPathUtils = import "LrPathUtils"

local State = { stop = false }
local logPath = LrPathUtils.child(_PLUGIN.path, "safelight.log")

function State.log(msg)
  -- Start over past 1 MB so the log can't grow forever.
  local attrs = LrFileUtils.fileAttributes(logPath)
  local mode = (attrs and attrs.fileSize and attrs.fileSize > 1024 * 1024) and "wb" or "ab"
  local fh = io.open(logPath, mode)
  if fh then
    fh:write(os.date("%Y-%m-%d %H:%M:%S  ") .. tostring(msg) .. "\n")
    fh:close()
  end
end

return State

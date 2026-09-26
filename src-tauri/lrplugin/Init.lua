local ok, err = pcall(require, "Service")
if not ok then
  local fh = io.open(_PLUGIN.path .. "/safelight.log", "ab")
  if fh then fh:write(os.date("%Y-%m-%d %H:%M:%S  ") .. "error: plug-in failed to start: " .. tostring(err) .. "\n") fh:close() end
end

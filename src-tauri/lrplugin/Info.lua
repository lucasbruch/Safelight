-- Installed by Safelight into Lightroom's Modules folder. Receives hand-offs
-- from Safelight through the inbox/ folder next to this file.
return {
  LrSdkVersion = 6.0,
  LrSdkMinimumVersion = 6.0,
  LrToolkitIdentifier = "app.safelight.handoff",
  LrPluginName = "Safelight",
  LrInitPlugin = "Init.lua",
  LrShutdownPlugin = "Shutdown.lua",
  -- Lightroom only honours LrForceInitPlugin for plug-ins with a menu item.
  LrForceInitPlugin = true,
  LrExportMenuItems = {
    { title = "Safelight: Check for Photos", file = "CheckNow.lua" },
  },
  VERSION = { major = 1, minor = 1, revision = 0 },
}

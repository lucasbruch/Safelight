-- Polls inbox/*.txt written by Safelight, adds exactly those photos to the
-- catalog in place, applies Safelight's stars/picks/keywords and shows them in
-- the "Safelight > <project>" collection. Writes <job>.done with the outcome.
local LrApplication = import "LrApplication"
local LrFileUtils = import "LrFileUtils"
local LrPathUtils = import "LrPathUtils"
local LrTasks = import "LrTasks"
local State = require "State"
local log = State.log

local inbox = LrPathUtils.child(_PLUGIN.path, "inbox")
local alive = LrPathUtils.child(_PLUGIN.path, "alive")

local function split(line)
  local t = {}
  for f in (line .. "\t"):gmatch("([^\t]*)\t") do t[#t + 1] = f end
  return t
end

local function parse(text)
  local job = { photos = {} }
  for line in text:gmatch("[^\r\n]+") do
    local f = split(line)
    if f[1] == "project" then job.project = f[2]
    elseif f[1] == "artist" then job.artist = f[2]
    elseif f[1] == "copyright" then job.copyright = f[2]
    elseif f[1] == "photo" then
      local kw = {}
      for i = 5, #f do if f[i] ~= "" then kw[#kw + 1] = f[i] end end
      job.photos[#job.photos + 1] = { path = f[2], rating = tonumber(f[3]) or 0, flag = tonumber(f[4]) or 0, keywords = kw }
    end
  end
  return job
end

local function readAll(path)
  local fh = io.open(path, "rb")
  if not fh then return nil end
  local s = fh:read("*a")
  fh:close()
  return s
end

local function write(path, text)
  local fh = io.open(path, "wb")
  if fh then fh:write(text) fh:close() end
end

local function set(photo, key, value)
  local ok, err = pcall(photo.setRawMetadata, photo, key, value)
  if not ok then log("could not set " .. key .. ": " .. tostring(err)) end
end

local function process(file)
  local job = parse(readAll(file) or "")
  LrFileUtils.delete(file)
  log(string.format("job %s: %d photos for %s", LrPathUtils.leafName(file), #job.photos, tostring(job.project)))
  local catalog = LrApplication.activeCatalog()
  local added, updated, failed = 0, 0, 0
  local found = {}

  -- Add in chunks so Lightroom stays responsive on large hand-offs.
  local i = 1
  while i <= #job.photos do
    local last = math.min(i + 99, #job.photos)
    catalog:withWriteAccessDo("Safelight: add photos", function()
      for j = i, last do
        local e = job.photos[j]
        local photo = catalog:findPhotoByPath(e.path)
        if photo then
          updated = updated + 1
        else
          local ok, res = LrTasks.pcall(catalog.addPhoto, catalog, e.path)
          if ok and res then
            photo = res
            added = added + 1
          else
            failed = failed + 1
            log("could not add " .. e.path .. ": " .. tostring(res))
          end
        end
        if photo then found[#found + 1] = { photo = photo, e = e } end
      end
    end, { timeout = 60 })
    i = last + 1
  end

  -- Metadata in its own pass, after Lightroom has read each file's own XMP.
  local collection
  catalog:withWriteAccessDo("Safelight: stars and picks", function()
    local keywords = {}
    local function keyword(name)
      if keywords[name] == nil then
        keywords[name] = catalog:createKeyword(name, {}, true, nil, true) or false
      end
      return keywords[name]
    end
    for _, x in ipairs(found) do
      local p, e = x.photo, x.e
      set(p, "rating", e.rating > 0 and e.rating or nil)
      set(p, "pickStatus", e.flag)
      if e.flag > 0 then
        set(p, "label", "Green")
      elseif p:getRawMetadata("colorNameForLabel") == "green" then
        set(p, "label", "")
      end
      for _, k in ipairs(e.keywords) do
        local kw = keyword(k)
        if kw then pcall(p.addKeyword, p, kw) end
      end
      if job.artist and job.artist ~= "" then set(p, "creator", job.artist) end
      if job.copyright and job.copyright ~= "" then
        set(p, "copyright", job.copyright)
        set(p, "copyrightState", "copyrighted")
      end
    end
    local parent = catalog:createCollectionSet("Safelight", nil, true)
    collection = catalog:createCollection(job.project or "Hand-off", parent, true)
    if collection then
      collection:removeAllPhotos()
      local list = {}
      for _, x in ipairs(found) do list[#list + 1] = x.photo end
      collection:addPhotos(list)
    end
  end, { timeout = 60 })

  if collection then pcall(catalog.setActiveSources, catalog, { collection }) end
  log(string.format("job done: %d added, %d updated, %d failed", added, updated, failed))
  write(LrPathUtils.replaceExtension(file, "done"), string.format("%d\t%d\t%d\n", added, updated, failed))
end

-- Handles every waiting job; returns how many there were. The background loop
-- and the menu item can both call this, so only one runs at a time.
local busy = false
local function runJobs()
  local jobs = {}
  for file in LrFileUtils.files(inbox) do
    if LrPathUtils.extension(file) == "txt" then jobs[#jobs + 1] = file end
  end
  for _, file in ipairs(jobs) do
    local ok, err = LrTasks.pcall(process, file)
    if not ok then
      log("error: " .. tostring(err))
      LrFileUtils.delete(file)
      write(LrPathUtils.replaceExtension(file, "done"), "error\t" .. tostring(err):gsub("[\t\r\n]", " ") .. "\n")
    end
  end
  return #jobs
end

local function tick()
  if busy then return 0 end
  busy = true
  -- Whatever happens in there, clear `busy`, or no job would ever run again.
  local ok, res = LrTasks.pcall(runJobs)
  busy = false
  if not ok then error(res) end
  return res
end

LrTasks.startAsyncTask(function()
  log("started (Lightroom " .. tostring(LrApplication.versionString()) .. ")")
  LrFileUtils.createAllDirectories(inbox)
  local beat = 0
  while not State.stop do
    if beat <= 0 then
      write(alive, tostring(os.time()))
      beat = 3
    end
    beat = beat - 1
    local ok, err = LrTasks.pcall(tick)
    if not ok then log("error: " .. tostring(err)) end
    LrTasks.sleep(1)
  end
end)

return { tick = tick }

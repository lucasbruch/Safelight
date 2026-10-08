//! Hand-off to Lightroom Classic and DaVinci Resolve Studio.
//!
//! Lightroom: a small bundled plug-in (see `lrplugin/`) adds exactly the chosen
//! photos in place and sets their stars, picks and keywords from Safelight.
//! Resolve: drive the Studio scripting API with Resolve's bundled `fuscript`
//! (Lua), so no Python install is needed.

use crate::meta::Kind;
use crate::project::{Item, Project};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub lightroom: bool,
    pub resolve: bool,
}

pub fn status() -> Status {
    Status { lightroom: lightroom_exe().is_some(), resolve: resolve_paths().is_some() }
}

fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.exists()).cloned()
}

fn program_files() -> Vec<PathBuf> {
    ["ProgramW6432", "ProgramFiles"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(PathBuf::from)
        .collect()
}

fn lightroom_exe() -> Option<PathBuf> {
    if cfg!(windows) {
        let c: Vec<PathBuf> = program_files()
            .into_iter()
            .map(|p| p.join("Adobe").join("Adobe Lightroom Classic").join("Lightroom.exe"))
            .collect();
        first_existing(&c)
    } else {
        first_existing(&[PathBuf::from("/Applications/Adobe Lightroom Classic/Adobe Lightroom Classic.app")])
    }
}

struct ResolvePaths {
    app: PathBuf,
    fuscript: PathBuf,
}

fn resolve_paths() -> Option<ResolvePaths> {
    if cfg!(windows) {
        program_files().into_iter().find_map(|p| {
            let dir = p.join("Blackmagic Design").join("DaVinci Resolve");
            let (app, fuscript) = (dir.join("Resolve.exe"), dir.join("fuscript.exe"));
            (app.exists() && fuscript.exists()).then_some(ResolvePaths { app, fuscript })
        })
    } else {
        let app = PathBuf::from("/Applications/DaVinci Resolve/DaVinci Resolve.app");
        let fuscript = app.join("Contents/Libraries/Fusion/fuscript");
        (app.exists() && fuscript.exists()).then_some(ResolvePaths { app, fuscript })
    }
}

/// What Safelight knows about a photo, handed to the Lightroom plug-in.
pub struct LrPhoto {
    pub path: PathBuf,
    pub rating: i64,
    pub flag: i64,
    pub keywords: Vec<String>,
}

/// The given items as Lightroom hand-offs (skipping moved rejects and missing files).
pub fn lr_photos(p: &Project, ids: &[i64]) -> Result<Vec<LrPhoto>> {
    let photos: Vec<LrPhoto> = ids
        .iter()
        .filter_map(|id| p.item(*id).ok())
        .filter(|it| !it.moved_to_rejected)
        .map(|it| LrPhoto {
            path: p.abs(&it.rel_path, false),
            rating: it.rating,
            flag: it.flag,
            keywords: it.keywords(),
        })
        .filter(|f| f.path.exists())
        .collect();
    anyhow::ensure!(!photos.is_empty(), "Nothing to send. Pick some photos first.");
    Ok(photos)
}

/// Starts an editor as an independent app: no console window, no shared output.
fn launch(mut cmd: Command, what: &str) -> Result<()> {
    use std::process::Stdio;
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    cmd.spawn().with_context(|| format!("starting {what}"))?;
    Ok(())
}

fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

const LR_PLUGIN: [(&str, &str); 6] = [
    ("Info.lua", include_str!("../lrplugin/Info.lua")),
    ("CheckNow.lua", include_str!("../lrplugin/CheckNow.lua")),
    ("Init.lua", include_str!("../lrplugin/Init.lua")),
    ("Service.lua", include_str!("../lrplugin/Service.lua")),
    ("State.lua", include_str!("../lrplugin/State.lua")),
    ("Shutdown.lua", include_str!("../lrplugin/Shutdown.lua")),
];

/// Lightroom Classic loads every plug-in in this folder at startup.
fn lr_modules_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Adobe").join("Lightroom").join("Modules"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Adobe/Lightroom/Modules"))
    }
}

/// Installs (or refreshes) the plug-in; returns its folder and whether any file changed.
fn install_lr_plugin() -> Result<(PathBuf, bool)> {
    let dir = lr_modules_dir()
        .ok_or_else(|| anyhow!("Couldn't find Lightroom's plug-in folder."))?
        .join("Safelight.lrplugin");
    std::fs::create_dir_all(dir.join("inbox")).with_context(|| format!("creating {}", dir.display()))?;
    let mut changed = false;
    for (name, body) in LR_PLUGIN {
        let f = dir.join(name);
        if std::fs::read_to_string(&f).ok().as_deref() != Some(body) {
            std::fs::write(&f, body).with_context(|| format!("writing {}", f.display()))?;
            changed = true;
        }
    }
    Ok((dir, changed))
}

/// Drops finished results and jobs nobody picked up within an hour, so an old
/// send doesn't surprise anyone the next time Lightroom starts.
fn prune_inbox(inbox: &Path) {
    let Ok(rd) = std::fs::read_dir(inbox) else { return };
    for e in rd.flatten() {
        let f = e.path();
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        let ext = f.extension().and_then(|x| x.to_str()).unwrap_or("");
        if ext == "done" || ext == "tmp" || (ext == "txt" && old) {
            let _ = std::fs::remove_file(&f);
        }
    }
}

/// The plug-in's last logged error, unless it has started cleanly since.
fn lr_plugin_error(dir: &Path) -> Option<String> {
    let log = std::fs::read_to_string(dir.join("safelight.log")).ok()?;
    let last = log.lines().rev().find(|l| l.contains("started (") || l.contains("error"))?;
    last.contains("error").then(|| last.to_string())
}

/// The plug-in touches `alive` every few seconds while Lightroom runs.
fn lr_plugin_alive(dir: &Path) -> bool {
    std::fs::metadata(dir.join("alive"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(10))
}

fn lightroom_running() -> bool {
    if cfg!(windows) {
        no_window(Command::new("tasklist").args(["/FI", "IMAGENAME eq Lightroom.exe", "/NH"]))
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("lightroom.exe"))
    } else {
        Command::new("pgrep").args(["-x", "Adobe Lightroom Classic"]).output().is_ok_and(|o| o.status.success())
    }
}

/// Tabs and line breaks separate fields in the inbox file.
fn field(s: &str) -> String {
    s.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect::<String>().trim().to_string()
}

fn photos_n(n: usize) -> String {
    format!("{n} photo{}", if n == 1 { "" } else { "s" })
}

/// Queues exactly these photos for the Safelight plug-in inside Lightroom, which adds
/// them in place and sets stars, picks and keywords itself. (Passing files to
/// Lightroom.exe opens its import on the whole folder instead, and Lightroom
/// ignores sidecar XMP for JPEG/HEIC.)
pub fn lightroom(project: &str, photos: &[LrPhoto], artist: &str, copyright: &str) -> Result<String> {
    let exe = lightroom_exe().ok_or_else(|| anyhow!("Lightroom Classic isn't installed on this computer."))?;
    let photos: Vec<&LrPhoto> = photos.iter().filter(|f| crate::meta::classify(&f.path) == Some(Kind::Photo)).collect();
    anyhow::ensure!(!photos.is_empty(), "Lightroom Classic only takes photos; your selection is all video.");
    let (dir, updated_plugin) = install_lr_plugin()?;
    let inbox = dir.join("inbox");
    prune_inbox(&inbox);

    let mut job = format!("project\t{}\nartist\t{}\ncopyright\t{}\n", field(project), field(artist), field(copyright));
    for p in &photos {
        job.push_str(&format!("photo\t{}\t{}\t{}", field(&p.path.to_string_lossy()), p.rating.clamp(0, 5), p.flag.signum()));
        let pick = (p.flag > 0).then_some(crate::xmp::PICK_KEYWORD);
        for k in p.keywords.iter().map(String::as_str).chain(pick) {
            job.push('\t');
            job.push_str(&field(k));
        }
        job.push('\n');
    }
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let (tmp, done) = (inbox.join(format!("{id}.tmp")), inbox.join(format!("{id}.done")));
    std::fs::write(&tmp, job)?;
    std::fs::rename(&tmp, inbox.join(format!("{id}.txt")))?;

    let n = photos.len();
    let collection = format!("Safelight \u{203a} {}", field(project));
    if lr_plugin_alive(&dir) {
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(60) {
            if let Ok(s) = std::fs::read_to_string(&done) {
                let _ = std::fs::remove_file(&done);
                let f: Vec<&str> = s.trim().split('\t').collect();
                if f.first() == Some(&"error") {
                    bail!("Lightroom couldn't take the photos: {}", f.get(1).unwrap_or(&""));
                }
                let num = |i: usize| f.get(i).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let (added, updated, failed) = (num(0), num(1), num(2));
                let mut msg = format!(
                    "Sent {} to Lightroom Classic, in the collection \u{201c}{collection}\u{201d}.",
                    photos_n(added + updated)
                );
                if updated > 0 {
                    msg.push_str(&format!(" {} were already in the catalog; their stars and picks were updated.", photos_n(updated)));
                }
                if failed > 0 {
                    msg.push_str(&format!(" Lightroom couldn't add {}.", photos_n(failed)));
                }
                return Ok(msg);
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        return Ok(format!("Lightroom Classic is still adding {}. They'll show up in \u{201c}{collection}\u{201d}.", photos_n(n)));
    }
    if lightroom_running() {
        if !updated_plugin {
            if let Some(e) = lr_plugin_error(&dir) {
                bail!("The Safelight plug-in in Lightroom Classic reported a problem: {e}");
            }
        }
        return Ok(format!(
            "Quit and reopen Lightroom Classic once so it loads the Safelight plug-in. The {} are queued and will appear in \u{201c}{collection}\u{201d}. If they don't, check that \u{201c}Safelight\u{201d} is enabled under File \u{2192} Plug-in Manager.",
            photos_n(n)
        ));
    }
    let cmd = if cfg!(windows) { Command::new(&exe) } else { let mut c = Command::new("open"); c.arg("-a").arg(&exe); c };
    launch(cmd, "Lightroom Classic")?;
    Ok(format!("Lightroom Classic is starting. The {} will appear in \u{201c}{collection}\u{201d} once it's open.", photos_n(n)))
}

fn lua_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

const RESOLVE_LUA: &str = r#"
local r = bmd.scriptapp("Resolve")
if not r then print("SAFELIGHT_ERR:NO_CONNECTION") return end
local pm = r:GetProjectManager()
local proj = pm:GetCurrentProject()
if not proj or proj:GetName() ~= PROJECT then
  -- Switching projects closes the open one; save it first so no edits are lost.
  if proj then pm:SaveProject() end
  proj = pm:LoadProject(PROJECT) or pm:CreateProject(PROJECT)
end
if not proj then print("SAFELIGHT_ERR:PROJECT") return end
local mp = proj:GetMediaPool()
-- Resolve's Lua lists are tables with numeric keys plus bookkeeping like "__flags".
local function items(t)
  local out = {}
  for k, v in pairs(t or {}) do
    if type(k) == "number" then table.insert(out, v) end
  end
  return out
end
local function sub(parent, name)
  for _, f in ipairs(items(parent:GetSubFolderList())) do
    if f:GetName() == name then return f end
  end
  return mp:AddSubFolder(parent, name)
end
local total = 0
for _, g in ipairs(GROUPS) do
  local folder = mp:GetRootFolder()
  for _, p in ipairs(g.bins) do folder = sub(folder, p) end
  local have = {}
  for _, c in ipairs(items(folder:GetClipList())) do
    have[string.lower(c:GetClipProperty("File Path") or "")] = true
  end
  local todo, info = {}, {}
  for _, f in ipairs(g.files) do
    if not have[string.lower(f.path)] then
      table.insert(todo, f.path)
      info[string.lower(f.path)] = f
    end
  end
  if #todo > 0 then
    mp:SetCurrentFolder(folder)
    for _, it in ipairs(items(mp:ImportMedia(todo))) do
      total = total + 1
      local f = info[string.lower(it:GetClipProperty("File Path") or "")]
      if f then
        if f.pick then it:AddFlag("Green") end
        if f.keywords ~= "" then it:SetMetadata("Keywords", f.keywords) end
        if f.comment ~= "" then it:SetMetadata("Comments", f.comment) end
      end
    end
  end
end
r:OpenPage(PAGE)
print("SAFELIGHT_OK:" .. total)
"#;

fn run_fuscript(fuscript: &Path, script: &Path) -> Result<String> {
    let out = no_window(Command::new(fuscript).arg("-l").arg("lua").arg(script))
        .output()
        .context("running Resolve's script interpreter")?;
    Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

pub fn resolve(p: &Project, items: &[Item]) -> Result<String> {
    let rp = resolve_paths().ok_or_else(|| anyhow!("DaVinci Resolve isn't installed on this computer."))?;
    let items: Vec<&Item> = items.iter().filter(|i| !i.moved_to_rejected).collect();
    anyhow::ensure!(!items.is_empty(), "Nothing to send. Pick some photos first.");

    // Bins mirror the folders: <date>/<camera>[/Video].
    let mut groups: BTreeMap<Vec<String>, Vec<&Item>> = BTreeMap::new();
    for it in &items {
        let mut parts: Vec<String> = it.rel_path.split('/').map(String::from).collect();
        parts.pop();
        groups.entry(parts).or_default().push(it);
    }
    let photos = items.iter().filter(|i| i.kind == Kind::Photo).count();
    let page = if photos * 2 >= items.len() { "photo" } else { "media" };

    let mut lua = format!("PROJECT = {}\nPAGE = {}\nGROUPS = {{\n", lua_str(&p.name()), lua_str(page));
    for (bins, its) in &groups {
        lua.push_str("  { bins = {");
        lua.push_str(&bins.iter().map(|b| lua_str(b)).collect::<Vec<_>>().join(", "));
        lua.push_str("}, files = {\n");
        for it in its {
            let path = p.abs(&it.rel_path, false);
            let keywords = it.keywords().join(", ");
            let comment = if it.rating > 0 { "\u{2605}".repeat(it.rating as usize) } else { String::new() };
            lua.push_str(&format!(
                "    {{ path = {}, pick = {}, keywords = {}, comment = {} }},\n",
                lua_str(&path.to_string_lossy()),
                it.flag == 1,
                lua_str(&keywords),
                lua_str(&comment)
            ));
        }
        lua.push_str("  } },\n");
    }
    lua.push_str("}\n");
    lua.push_str(RESOLVE_LUA);

    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let script = std::env::temp_dir().join(format!("safelight_resolve_{}_{stamp}.lua", std::process::id()));
    std::fs::write(&script, lua)?;

    // Start Resolve if needed and wait for its scripting server.
    let mut out = run_fuscript(&rp.fuscript, &script)?;
    if out.contains("SAFELIGHT_ERR:NO_CONNECTION") {
        let cmd = if cfg!(windows) { Command::new(&rp.app) } else { let mut c = Command::new("open"); c.arg("-a").arg(&rp.app); c };
        launch(cmd, "DaVinci Resolve")?;
        let t0 = Instant::now();
        while out.contains("SAFELIGHT_ERR:NO_CONNECTION") && t0.elapsed() < Duration::from_secs(150) {
            std::thread::sleep(Duration::from_secs(3));
            out = run_fuscript(&rp.fuscript, &script)?;
        }
    }
    let _ = std::fs::remove_file(&script);

    if let Some(n) = out.lines().find_map(|l| l.trim().strip_prefix("SAFELIGHT_OK:")) {
        let n: usize = n.trim().parse().unwrap_or(0);
        return Ok(if n == 0 {
            format!("Everything was already in the Resolve project \u{201c}{}\u{201d}.", p.name())
        } else {
            format!("Added {n} file{} to the Resolve project \u{201c}{}\u{201d}.", if n == 1 { "" } else { "s" }, p.name())
        });
    }
    if out.contains("SAFELIGHT_ERR:NO_CONNECTION") {
        bail!("Safelight couldn't talk to Resolve. In Resolve, open Preferences \u{2192} System \u{2192} General, set \u{201c}External scripting using\u{201d} to \u{201c}Local\u{201d}, restart Resolve and try again. (This needs DaVinci Resolve Studio.)");
    }
    if out.contains("SAFELIGHT_ERR:PROJECT") {
        bail!("Resolve couldn't create or open the project \u{201c}{}\u{201d}. Close any open dialogs in Resolve and try again.", p.name());
    }
    bail!("Resolve didn't confirm the import. Output: {}", out.trim())
}

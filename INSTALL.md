# Installing Safelight

Safelight runs on **Windows 10/11 (64-bit)** and **macOS 11 Big Sur or newer** (Apple Silicon and Intel).

- [Download](#download)
- [Windows](#windows)
- [macOS](#macos)
- [First start](#first-start)
- [Lightroom Classic and DaVinci Resolve](#lightroom-classic-and-davinci-resolve)
- [Updating and uninstalling](#updating-and-uninstalling)
- [Troubleshooting](#troubleshooting)
- [For maintainers: publishing a release](#for-maintainers-publishing-a-release)

## Download

Open the repository's **[Releases](https://github.com/lucasbruch/Safelight/releases)** page and, under the newest
release, expand **Assets**:

| Your computer | Download |
| --- | --- |
| Windows | `Safelight_<version>_x64-setup.exe` (recommended) or `Safelight_<version>_x64_en-US.msi` |
| Mac (any) | `Safelight_<version>_universal.dmg` |

> The repository is private, so the Releases page is only visible to people who have been given access to it.

Safelight isn't code-signed yet. Windows and macOS will warn you the first time you open it. The steps below show
how to get past that warning. You only have to do it once per version.

## Windows

1. Double-click **`Safelight_<version>_x64-setup.exe`**.
2. If **"Windows protected your PC"** appears, click **More info**, then **Run anyway**.
   (This is Microsoft SmartScreen reacting to an unsigned app. It isn't a virus warning.)
3. Follow the installer. It installs for your user account only and doesn't need administrator rights.
   The `.msi` does the same for all users on the computer and needs administrator rights. Use it only if you manage several PCs.
4. Start **Safelight** from the Start menu.

Safelight needs Microsoft Edge **WebView2**, which Windows 10 and 11 normally have already. If it's missing, the
installer downloads it, so stay online during installation.

Everything Safelight needs (ExifTool, ffmpeg, LibRaw, ONNX Runtime) is included. There's nothing else to install.

## macOS

1. Double-click **`Safelight_<version>_universal.dmg`** and drag **Safelight** into **Applications**.
2. Open **Applications** and double-click **Safelight**. macOS will refuse the first time because the app isn't
   signed by an identified developer:
   - **macOS 15 Sequoia and newer:** click **Done**, open **System Settings → Privacy & Security**, scroll down to
     *"Safelight" was blocked…* and click **Open Anyway**, then confirm with your password.
   - **macOS 11–14:** right-click (or Control-click) **Safelight** in Applications, choose **Open**, then **Open** again.
3. If macOS instead says **"Safelight is damaged and can't be opened"**, the app isn't damaged. macOS says this about
   unsigned apps downloaded from the internet. Open **Terminal** and run:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Safelight.app
   ```

   Then open Safelight again.
4. **Apple Silicon Macs (M1 and newer):** the bundled ffmpeg, which makes video previews, is an Intel program, so it needs
   Apple's Rosetta. If you've never installed Rosetta, run this once in Terminal:

   ```bash
   softwareupdate --install-rosetta --agree-to-license
   ```

   Without it, photos work normally, but clips show *"This clip can't be played in Safelight"*.

## First start

1. Open **Settings** (gear icon, top right):
   - **Projects folder**: where new projects go. Defaults to `Pictures/Safelight`.
   - **Backup copy** (optional): a second drive or NAS that every import is also copied to and verified on.
   - **Your name / Copyright**: written into your photos' metadata for Lightroom.
2. **AI helper** (optional): in Settings, click **Download** to fetch the face, blink and content models (~135 MB).
   They run on your computer; nothing is uploaded. Sharpness, exposure and horizon checks work without them.
   On **Intel Macs** the models aren't available; the other checks still run.
3. Insert a camera card. Safelight notices it and asks for a project name. You can also use **Import from a folder…**.

## Lightroom Classic and DaVinci Resolve

- **Lightroom Classic:** the first time you use **Send… → Lightroom Classic**, Safelight installs a small plug-in into
  Lightroom. If Lightroom is already open, quit and reopen it once. If photos don't arrive, check that **Safelight** is
  enabled under **File → Plug-in Manager**.
- **DaVinci Resolve:** this needs **DaVinci Resolve Studio** (the free version has no scripting). In Resolve, open
  **Preferences → System → General**, set **External scripting using** to **Local**, and restart Resolve.

## Updating and uninstalling

**Updating:** download the new version and install it over the old one, the same way as above. Your projects,
ratings and settings are kept.

**Uninstalling:**

- **Windows:** **Settings → Apps → Installed apps → Safelight → Uninstall**.
- **macOS:** drag **Safelight** from Applications to the Trash.

Uninstalling never touches your projects. To also remove Safelight's settings, import history and downloaded AI models,
delete this folder:

- Windows: `%APPDATA%\com.grabit.app`
- macOS: `~/Library/Application Support/com.grabit.app`

(`com.grabit.app` is the app's old internal name; it's kept so existing installs keep their settings.)

## Troubleshooting

| Problem | What to do |
| --- | --- |
| Windows: nothing happens after "Run anyway" | Look for the installer window behind other windows, or run the `.exe` again. |
| macOS: "can't be opened because Apple cannot check it" | Follow step 2 of [macOS](#macos) (Open Anyway / right-click → Open). |
| macOS: "is damaged and can't be opened" | Run the `xattr` command from step 3 of [macOS](#macos). |
| Clips won't play on an Apple Silicon Mac | Install Rosetta (step 4 of [macOS](#macos)), then reopen the project. |
| A card isn't detected | It needs a `DCIM` folder (or `PRIVATE/M4ROOT` for Sony video). Drives holding your projects or backup folder are ignored on purpose. Use **Import from a folder…** for anything else. |
| "Couldn't read the photos' metadata (ExifTool failed)" | Reinstall Safelight. On a Mac, also run the `xattr` command above. |
| AI helper download fails | Check your internet connection and try again. A download that arrives damaged is discarded automatically. |

## For maintainers: publishing a release

Releases are built by GitHub Actions (`.github/workflows/release.yml`) on Windows and macOS runners:

1. Bump `version` in `src-tauri/tauri.conf.json` and `package.json` (for example `0.2.0`), then commit.
2. Tag and push:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. When the **release** workflow finishes (Actions tab), open **Releases**. There's a **draft** with the `.exe`, `.msi` and
   `.dmg` attached. Add release notes and click **Publish release**.

You can also start the workflow by hand from **Actions → release → Run workflow**.

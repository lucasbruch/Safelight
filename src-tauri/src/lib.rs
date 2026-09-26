mod ai;
mod card;
mod commands;
mod copy;
mod exiftool;
mod handoff;
mod ingest;
mod jobs;
mod ledger;
mod meta;
mod preview;
mod project;
mod settings;
mod state;
mod video;
mod xmp;

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// On Windows, put Safelight in a job object that kills every helper process
/// (ExifTool, ffmpeg, LibRaw) when Safelight exits, even if it's killed. Editors we
/// launch (Lightroom, Resolve) break away from the job and keep running.
#[cfg(windows)]
fn kill_helpers_on_exit() {
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        // The handle is intentionally leaked: it closes when the process ends.
        AssignProcessToJobObject(job, GetCurrentProcess());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    kill_helpers_on_exit();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let settings = settings::Settings::load(&data_dir);
            let exe = exiftool::locate(app.path().resource_dir().ok());
            let exif = Arc::new(exiftool::ExifTool::new(exe, 3));
            preview::set_raw_decoder(exiftool::locate_resource(
                app.path().resource_dir().ok().as_deref(),
                "resources/libraw/windows/dcraw_emu.exe",
            ));
            video::set_ffmpeg(exiftool::locate_resource(
                app.path().resource_dir().ok().as_deref(),
                if cfg!(windows) { "resources/ffmpeg/windows/ffmpeg.exe" } else { "resources/ffmpeg/macos/ffmpeg" },
            ));
            let ledger = Arc::new(ledger::Ledger::open(&data_dir.join("ledger.sqlite"))?);
            let runtime = exiftool::locate_resource(
                app.path().resource_dir().ok().as_deref(),
                if cfg!(windows) { "resources/onnxruntime/windows/onnxruntime.dll" } else { "resources/onnxruntime/macos/libonnxruntime.dylib" },
            );
            let analyzer = Arc::new(ai::Analyzer::new(settings.ai_enabled, data_dir.join("models"), runtime));
            let jobs = jobs::Jobs::start(app.handle().clone(), exif.clone(), analyzer.clone());
            let state = Arc::new(state::AppState {
                app: app.handle().clone(),
                data_dir,
                settings: Mutex::new(settings),
                xmp: xmp::Writer::start(exif.clone()),
                exif,
                ledger,
                jobs,
                analyzer,
                projects: Mutex::new(HashMap::new()),
                scans: Mutex::new(HashMap::new()),
                import: Mutex::new(None),
                cancel_import: Arc::new(AtomicBool::new(false)),
            });
            app.manage(state.clone());
            watch_cards(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_cards,
            commands::scan_source,
            commands::start_import,
            commands::cancel_import,
            commands::import_status,
            commands::list_projects,
            commands::open_project,
            commands::set_rating,
            commands::set_flag,
            commands::move_rejects,
            commands::trash_rejects,
            commands::full_image,
            commands::eject_card,
            commands::rerun_ai,
            commands::request_proxy,
            commands::ai_status,
            commands::download_models,
            commands::handoff_status,
            commands::send_to_lightroom,
            commands::send_to_resolve,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Safelight");
}

/// Polls for camera cards and tells the UI when the set changes.
fn watch_cards(app: tauri::AppHandle, state: Arc<state::AppState>) {
    std::thread::Builder::new()
        .name("card-watch".into())
        .spawn(move || {
            let mut last = vec![];
            loop {
                let now = card::detect(&state.card_exclusions());
                if now != last {
                    let _ = app.emit("cards-changed", &now);
                    last = now;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("spawn card watcher");
}

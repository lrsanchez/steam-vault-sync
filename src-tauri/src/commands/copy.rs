use serde::Serialize;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB
const PROGRESS_INTERVAL_MS: u128 = 500;
const PAUSE_POLL_MS: u64 = 250;
const DEFAULT_TOOLTIP: &str = "Steam Vault Sync";

/// Shared control flags for the in-flight copy. Held in `tauri::State` so
/// the pause/resume/cancel commands can flip them while the blocking copy
/// thread polls them between chunks.
pub struct CopyControl {
    pub paused: AtomicBool,
    pub cancelled: AtomicBool,
    pub title: String,
}

#[derive(Default)]
pub struct CopyControlState(pub Mutex<Option<Arc<CopyControl>>>);

fn set_tray_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(crate::TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CopyProgress {
    copied_bytes: u64,
    total_bytes: u64,
    speed_bps: u64,
    eta_seconds: u64,
}

#[tauri::command]
pub fn pause_copy(
    app_handle: AppHandle,
    state: State<'_, CopyControlState>,
) -> Result<(), String> {
    if let Some(c) = state.0.lock().unwrap().as_ref() {
        c.paused.store(true, Ordering::Relaxed);
        set_tray_tooltip(&app_handle, &format!("{} — paused", c.title));
        let _ = app_handle.emit("copy://paused", &c.title);
    }
    Ok(())
}

#[tauri::command]
pub fn resume_copy(
    app_handle: AppHandle,
    state: State<'_, CopyControlState>,
) -> Result<(), String> {
    if let Some(c) = state.0.lock().unwrap().as_ref() {
        c.paused.store(false, Ordering::Relaxed);
        let _ = app_handle.emit("copy://resumed", &c.title);
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_copy(state: State<'_, CopyControlState>) -> Result<(), String> {
    if let Some(c) = state.0.lock().unwrap().as_ref() {
        c.cancelled.store(true, Ordering::Relaxed);
        // Unpause so the chunk loop notices the cancel flag immediately
        // instead of sitting in the pause-poll sleep.
        c.paused.store(false, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn copy_game(
    app_handle: AppHandle,
    state: State<'_, CopyControlState>,
    source_path: String,
    dest_library_path: String,
    game_title: String,
) -> Result<(), String> {
    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err(format!("Source not found: {}", source.display()));
    }

    let folder_name = source
        .file_name()
        .ok_or_else(|| "Invalid source path".to_string())?
        .to_string_lossy()
        .to_string();

    let dest_root = PathBuf::from(&dest_library_path);
    let dest = dest_root.join(&folder_name);

    fs::create_dir_all(&dest_root).map_err(|e| e.to_string())?;

    let total_bytes = total_size(&source);
    let copied = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let last_emit = Arc::new(Mutex::new(Instant::now()));

    let control = Arc::new(CopyControl {
        paused: AtomicBool::new(false),
        cancelled: AtomicBool::new(false),
        title: game_title.clone(),
    });
    *state.0.lock().unwrap() = Some(control.clone());

    set_tray_tooltip(&app_handle, &format!("Copying {} — starting…", game_title));

    let copy_result = tokio::task::spawn_blocking({
        let app_handle = app_handle.clone();
        let source = source.clone();
        let dest = dest.clone();
        let title = game_title.clone();
        let copied = copied.clone();
        let last_emit = last_emit.clone();
        let control = control.clone();
        move || -> Result<(), String> {
            copy_dir_recursive(
                &source,
                &dest,
                &app_handle,
                &title,
                &copied,
                total_bytes,
                &start,
                &last_emit,
                &control,
            )
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    *state.0.lock().unwrap() = None;

    if let Err(e) = copy_result {
        let _ = fs::remove_dir_all(&dest);
        set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
        let event = if control.cancelled.load(Ordering::Relaxed) {
            "copy://cancelled"
        } else {
            "copy://error"
        };
        let _ = app_handle.emit(
            event,
            serde_json::json!({ "title": game_title, "error": e }),
        );
        return Err(e);
    }

    set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
    let _ = app_handle.emit(
        "copy://done",
        serde_json::json!({ "title": game_title, "destPath": dest.to_string_lossy() }),
    );

    Ok(())
}

fn copy_dir_recursive(
    source: &Path,
    dest: &Path,
    app_handle: &AppHandle,
    game_title: &str,
    copied: &AtomicU64,
    total_bytes: u64,
    start: &Instant,
    last_emit: &Mutex<Instant>,
    control: &Arc<CopyControl>,
) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        if control.cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(
                &from,
                &to,
                app_handle,
                game_title,
                copied,
                total_bytes,
                start,
                last_emit,
                control,
            )?;
        } else {
            copy_file_chunked(
                &from,
                &to,
                app_handle,
                game_title,
                copied,
                total_bytes,
                start,
                last_emit,
                control,
            )?;
        }
    }
    Ok(())
}

fn copy_file_chunked(
    source: &Path,
    dest: &Path,
    app_handle: &AppHandle,
    game_title: &str,
    copied: &AtomicU64,
    total_bytes: u64,
    start: &Instant,
    last_emit: &Mutex<Instant>,
    control: &Arc<CopyControl>,
) -> Result<(), String> {
    let mut src_file = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut dst_file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; CHUNK_SIZE];

    loop {
        // Honor cancel as soon as possible.
        if control.cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }
        // Sit in a short polling sleep while paused so the OS doesn't burn
        // a core on a busy spin. The chunk granularity gives sub-second
        // pause latency without sacrificing throughput.
        while control.paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(PAUSE_POLL_MS));
            if control.cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled by user".to_string());
            }
        }

        let n = src_file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        dst_file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        let new_total = copied.fetch_add(n as u64, Ordering::Relaxed) + n as u64;

        let mut last = last_emit.lock().unwrap();
        if last.elapsed().as_millis() >= PROGRESS_INTERVAL_MS {
            *last = Instant::now();
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed_bps = (new_total as f64 / elapsed) as u64;
            let remaining = total_bytes.saturating_sub(new_total);
            let eta = if speed_bps > 0 {
                remaining / speed_bps
            } else {
                0
            };
            let pct = if total_bytes > 0 {
                (new_total as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };
            let mbps = (speed_bps as f64) / 1_048_576.0;
            set_tray_tooltip(
                app_handle,
                &format!(
                    "Copying {} — {:.0}%  ({:.0} MB/s, ETA {}s)",
                    game_title, pct, mbps, eta
                ),
            );
            let _ = app_handle.emit(
                "copy://progress",
                CopyProgress {
                    copied_bytes: new_total,
                    total_bytes,
                    speed_bps,
                    eta_seconds: eta,
                },
            );
        }
    }
    Ok(())
}

fn total_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

#[tauri::command]
pub async fn remove_local_game(
    library_path: String,
    folder_name: String,
    app_id: Option<String>,
) -> Result<(), String> {
    let common = PathBuf::from(&library_path);
    let target = common.join(&folder_name);
    if !target.exists() {
        return Err(format!("Folder not found: {}", target.display()));
    }
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;

    // Clean up the Steam-side metadata so the game doesn't appear half-
    // installed on next Steam launch. `library_path` points at the
    // `steamapps/common` folder; the manifest + sibling artifacts live in
    // its parent `steamapps/`.
    if let (Some(steamapps), Some(id)) = (common.parent(), app_id.as_deref()) {
        let manifest = steamapps.join(format!("appmanifest_{}.acf", id));
        let _ = fs::remove_file(&manifest);

        let downloading = steamapps.join("downloading").join(id);
        if downloading.exists() {
            let _ = fs::remove_dir_all(&downloading);
        }

        let workshop = steamapps.join("workshop").join("content").join(id);
        if workshop.exists() {
            let _ = fs::remove_dir_all(&workshop);
        }

        let compatdata = steamapps.join("compatdata").join(id);
        if compatdata.exists() {
            let _ = fs::remove_dir_all(&compatdata);
        }
    }

    Ok(())
}

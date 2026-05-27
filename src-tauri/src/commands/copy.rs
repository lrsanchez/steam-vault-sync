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
    app_id: Option<String>,
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
                "Copying",
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

    // Copy the source's appmanifest_<appid>.acf into the destination
    // Steam library's steamapps folder. Without this, Steam treats the
    // copied files as "loose files of unknown version" and performs a
    // full integrity check (reading every file's bytes to hash) before
    // it can apply a patch — wasting tens of minutes on big games. With
    // the manifest in place, Steam knows the buildid and downloads
    // only the delta to the cloud version.
    if let Some(id) = app_id.as_deref() {
        let source_steamapps = source.parent().and_then(|p| p.parent());
        let dest_steamapps = dest_root.parent();
        if let (Some(ss), Some(ds)) = (source_steamapps, dest_steamapps) {
            let acf_name = format!("appmanifest_{}.acf", id);
            let src_acf = ss.join(&acf_name);
            let dst_acf = ds.join(&acf_name);
            if src_acf.exists() {
                let _ = fs::copy(&src_acf, &dst_acf);
            }
        }
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
    verb: &str,
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
                verb,
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
                verb,
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
    verb: &str,
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
                    "{} {} — {:.0}%  ({:.0} MB/s, ETA {}s)",
                    verb, game_title, pct, mbps, eta
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

/// Push a locally-installed, Steam-updated game back to the vault.
/// Used in the "staged update" workflow: user copies vault game → local
/// (fast sequential read), lets Steam patch the local copy on internal
/// NVMe (fast random I/O), then pushes the updated copy back to the
/// vault. Drastically faster than Steam patching the vault in-place
/// over USB.
///
/// Write strategy: copy into `<folder>.partial` in vault common, then
/// remove the old folder and rename `.partial` → final. If the copy is
/// cancelled or fails partway, the original vault folder is untouched
/// and the `.partial` is cleaned up.
#[tauri::command]
pub async fn push_to_vault(
    app_handle: AppHandle,
    state: State<'_, CopyControlState>,
    local_lib_path: String,
    vault_lib_path: String,
    folder_name: String,
    app_id: Option<String>,
    game_title: String,
) -> Result<(), String> {
    let source = PathBuf::from(&local_lib_path).join(&folder_name);
    if !source.exists() {
        return Err(format!(
            "Local game folder not found: {}",
            source.display()
        ));
    }

    let vault_common = PathBuf::from(&vault_lib_path);
    let dest_final = vault_common.join(&folder_name);
    let dest_partial = vault_common.join(format!("{}.partial", folder_name));

    // Clean up any leftover .partial from a previous failed attempt.
    if dest_partial.exists() {
        fs::remove_dir_all(&dest_partial).map_err(|e| e.to_string())?;
    }

    fs::create_dir_all(&vault_common).map_err(|e| e.to_string())?;

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

    set_tray_tooltip(
        &app_handle,
        &format!("Updating vault: {} — starting…", game_title),
    );

    let copy_result = tokio::task::spawn_blocking({
        let app_handle = app_handle.clone();
        let source = source.clone();
        let dest = dest_partial.clone();
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
                "Updating vault:",
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
        let _ = fs::remove_dir_all(&dest_partial);
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

    // Atomic swap: remove old vault folder, then rename partial → final.
    // Brief vulnerability window (a few ms on SSD) where neither folder
    // exists at the canonical name. If the app crashes here, the
    // `.partial` survives and the user can recover by renaming it.
    if dest_final.exists() {
        if let Err(e) = fs::remove_dir_all(&dest_final) {
            set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
            return Err(format!(
                "Vault copy updated successfully but old folder couldn't be removed: {}. The new copy is at {}",
                e,
                dest_partial.display()
            ));
        }
    }
    if let Err(e) = fs::rename(&dest_partial, &dest_final) {
        set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
        return Err(format!("Failed to rename partial to final: {}", e));
    }

    // Copy the updated appmanifest_<appid>.acf so the vault's buildid
    // matches the new install. Without this, the next scan would still
    // see the old buildid.
    if let Some(id) = app_id.as_deref() {
        let local_steamapps = PathBuf::from(&local_lib_path).parent().map(|p| p.to_path_buf());
        let vault_steamapps = vault_common.parent().map(|p| p.to_path_buf());
        if let (Some(ls), Some(vs)) = (local_steamapps, vault_steamapps) {
            let acf_name = format!("appmanifest_{}.acf", id);
            let src_acf = ls.join(&acf_name);
            let dst_acf = vs.join(&acf_name);
            if src_acf.exists() {
                let _ = fs::copy(&src_acf, &dst_acf);
            }
        }
    }

    set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
    let _ = app_handle.emit(
        "vault-push://done",
        serde_json::json!({ "title": game_title, "destPath": dest_final.to_string_lossy() }),
    );

    Ok(())
}

/// Copy a game folder from one vault to another. Same chunked
/// pipeline as `push_to_vault` (atomic .partial-then-rename, pause/
/// resume/cancel, tray tooltip). Also copies the source's
/// appmanifest_*.acf into the destination's steamapps folder so the
/// destination vault is immediately recognized by Steam (and by our
/// own next rescan).
///
/// Note: USB-to-USB copy speed is bounded by the slower of the two
/// drives — and worse if they share a USB controller.
#[tauri::command]
pub async fn copy_to_vault(
    app_handle: AppHandle,
    state: State<'_, CopyControlState>,
    source_lib_path: String,
    dest_lib_path: String,
    folder_name: String,
    app_id: Option<String>,
    game_title: String,
) -> Result<(), String> {
    let source = PathBuf::from(&source_lib_path).join(&folder_name);
    if !source.exists() {
        return Err(format!(
            "Source game folder not found: {}",
            source.display()
        ));
    }

    let dest_common = PathBuf::from(&dest_lib_path);
    let dest_final = dest_common.join(&folder_name);
    let dest_partial = dest_common.join(format!("{}.partial", folder_name));

    if dest_partial.exists() {
        fs::remove_dir_all(&dest_partial).map_err(|e| e.to_string())?;
    }

    fs::create_dir_all(&dest_common).map_err(|e| e.to_string())?;

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

    set_tray_tooltip(
        &app_handle,
        &format!("Copying to vault: {} — starting…", game_title),
    );

    let copy_result = tokio::task::spawn_blocking({
        let app_handle = app_handle.clone();
        let source = source.clone();
        let dest = dest_partial.clone();
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
                "Copying to vault:",
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
        let _ = fs::remove_dir_all(&dest_partial);
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

    if dest_final.exists() {
        if let Err(e) = fs::remove_dir_all(&dest_final) {
            set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
            return Err(format!(
                "Copy completed but old destination folder couldn't be removed: {}. New copy is at {}",
                e,
                dest_partial.display()
            ));
        }
    }
    if let Err(e) = fs::rename(&dest_partial, &dest_final) {
        set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
        return Err(format!("Failed to rename partial to final: {}", e));
    }

    // Copy the source vault's appmanifest into the destination
    // steamapps folder so Steam (and our rescan) recognizes the new
    // install immediately.
    if let Some(id) = app_id.as_deref() {
        let source_steamapps = PathBuf::from(&source_lib_path).parent().map(|p| p.to_path_buf());
        let dest_steamapps = dest_common.parent().map(|p| p.to_path_buf());
        if let (Some(ss), Some(ds)) = (source_steamapps, dest_steamapps) {
            let acf_name = format!("appmanifest_{}.acf", id);
            let src_acf = ss.join(&acf_name);
            let dst_acf = ds.join(&acf_name);
            if src_acf.exists() {
                let _ = fs::copy(&src_acf, &dst_acf);
            }
        }
    }

    set_tray_tooltip(&app_handle, DEFAULT_TOOLTIP);
    let _ = app_handle.emit(
        "vault-push://done",
        serde_json::json!({ "title": game_title, "destPath": dest_final.to_string_lossy() }),
    );

    Ok(())
}

/// Permanently delete a game from a vault. Removes:
///   - the game folder under `steamapps/common/`
///   - the matching `appmanifest_<appid>.acf`
///   - the row from `vaultsync.db`
///   - the AppID entry from `libraryfolders.vdf`'s vault apps block
///
/// Does NOT touch any local install on the user's PC. Caller MUST
/// confirm with the user first — this is irreversible from VaultSync's
/// side (the game can be reinstalled via Steam later if the user wants
/// it back in the vault).
#[tauri::command]
pub async fn delete_from_vault(
    drive_letter: String,
    folder_name: String,
    app_id: Option<String>,
) -> Result<(), String> {
    let common = super::steam_common_path(&drive_letter);
    let target = common.join(&folder_name);

    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|e| format!("Failed to remove vault game folder: {}", e))?;
    }

    if let Some(id) = app_id.as_deref() {
        if let Some(steamapps) = common.parent() {
            let manifest = steamapps.join(format!("appmanifest_{}.acf", id));
            if manifest.exists() {
                let _ = fs::remove_file(&manifest);
            }
        }
    }

    // Remove the row from the vault's vaultsync.db so the catalog
    // matches reality on next launch.
    let conn = super::catalog::open(&drive_letter)?;
    conn.execute(
        "DELETE FROM games WHERE folder_name = ?1",
        rusqlite::params![folder_name],
    )
    .map_err(|e| e.to_string())?;

    // Tell Steam this AppID is no longer in the vault library.
    if let Some(id) = app_id.as_deref() {
        let _ = super::vdf_isolation::permanently_strip_app_from_libraryfolders(
            &drive_letter,
            id,
        );
    }

    Ok(())
}

/// Temporarily rename the vault's `appmanifest_<appid>.acf` so Steam
/// can no longer see the vault install while we patch the local copy.
/// Without this, Steam picks whichever install it found first (often
/// the vault) when handling `steam://install/<appid>`, defeating the
/// whole "stage to fast NVMe" workflow.
///
/// The renamed sentinel lives next to the original as
/// `appmanifest_<appid>.acf.vaultsync-hidden`. Caller is responsible
/// for either restoring it (`restore_vault_manifest`) on cancel/error
/// or discarding it (`discard_hidden_vault_manifest`) after the
/// push-back has written a fresh manifest to the same location.
#[tauri::command]
pub async fn hide_vault_manifest(
    vault_lib_path: String,
    app_id: String,
) -> Result<bool, String> {
    let common = PathBuf::from(&vault_lib_path);
    let steamapps = common
        .parent()
        .ok_or_else(|| format!("Cannot derive steamapps from {}", common.display()))?;
    let manifest = steamapps.join(format!("appmanifest_{}.acf", app_id));
    let hidden = steamapps.join(format!("appmanifest_{}.acf.vaultsync-hidden", app_id));

    // If a stale hidden file from a previous run exists, remove it so
    // the rename below doesn't fail.
    if hidden.exists() {
        let _ = fs::remove_file(&hidden);
    }

    if !manifest.exists() {
        // Vault doesn't have a manifest for this AppID — nothing to hide.
        return Ok(false);
    }

    fs::rename(&manifest, &hidden).map_err(|e| {
        format!(
            "Failed to hide vault manifest at {}: {}",
            manifest.display(),
            e
        )
    })?;
    Ok(true)
}

/// Restore the previously-hidden vault manifest. Used when the auto-
/// update workflow is cancelled or fails before push-back has written
/// a fresh manifest.
#[tauri::command]
pub async fn restore_vault_manifest(
    vault_lib_path: String,
    app_id: String,
) -> Result<(), String> {
    let common = PathBuf::from(&vault_lib_path);
    let steamapps = common
        .parent()
        .ok_or_else(|| format!("Cannot derive steamapps from {}", common.display()))?;
    let manifest = steamapps.join(format!("appmanifest_{}.acf", app_id));
    let hidden = steamapps.join(format!("appmanifest_{}.acf.vaultsync-hidden", app_id));

    if !hidden.exists() {
        return Ok(());
    }
    // If the real manifest got recreated for some reason (e.g.,
    // partial push-back wrote one), prefer the existing real one and
    // discard the backup.
    if manifest.exists() {
        let _ = fs::remove_file(&hidden);
        return Ok(());
    }
    fs::rename(&hidden, &manifest)
        .map_err(|e| format!("Failed to restore vault manifest: {}", e))
}

/// Delete the hidden backup after a successful push-back has written
/// a fresh manifest to the original location.
#[tauri::command]
pub async fn discard_hidden_vault_manifest(
    vault_lib_path: String,
    app_id: String,
) -> Result<(), String> {
    let common = PathBuf::from(&vault_lib_path);
    let steamapps = common
        .parent()
        .ok_or_else(|| format!("Cannot derive steamapps from {}", common.display()))?;
    let hidden = steamapps.join(format!("appmanifest_{}.acf.vaultsync-hidden", app_id));
    if hidden.exists() {
        let _ = fs::remove_file(&hidden);
    }
    Ok(())
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

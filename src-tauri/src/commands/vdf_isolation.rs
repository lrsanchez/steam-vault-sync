use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const STEAM_LIBRARYFOLDERS_VDF: &str =
    "C:\\Program Files (x86)\\Steam\\steamapps\\libraryfolders.vdf";
const STEAM_EXIT_TIMEOUT_SECS: u64 = 30;

fn libraryfolders_vdf_path() -> PathBuf {
    PathBuf::from(STEAM_LIBRARYFOLDERS_VDF)
}

fn vdf_backup_path() -> PathBuf {
    PathBuf::from(format!("{}.vaultsync-backup", STEAM_LIBRARYFOLDERS_VDF))
}

fn vault_hidden_manifest_path(vault_lib_path: &str, app_id: &str) -> Option<PathBuf> {
    let common = PathBuf::from(vault_lib_path);
    let steamapps = common.parent()?.to_path_buf();
    Some(steamapps.join(format!("appmanifest_{}.acf.vaultsync-hidden", app_id)))
}

fn vault_manifest_path(vault_lib_path: &str, app_id: &str) -> Option<PathBuf> {
    let common = PathBuf::from(vault_lib_path);
    let steamapps = common.parent()?.to_path_buf();
    Some(steamapps.join(format!("appmanifest_{}.acf", app_id)))
}

// ---- Steam process control ----

fn is_steam_running() -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        p.name()
            .to_string_lossy()
            .eq_ignore_ascii_case("steam.exe")
    })
}

fn fire_steam_exit() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "steam://exit"])
            .spawn()
            .map_err(|e| format!("Failed to fire steam://exit: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Steam URIs: Windows only".to_string())
    }
}

async fn wait_for_steam_exit(timeout_secs: u64) -> Result<(), String> {
    let start = Instant::now();
    while is_steam_running() {
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(format!(
                "Steam did not exit within {} seconds — it may be busy with another download. \
                 Close Steam manually and retry, or pick the manual ↑ Vault workflow instead.",
                timeout_secs
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}

// ---- libraryfolders.vdf surgery ----

/// Remove the AppID entry from the given vault library's `apps` block
/// in libraryfolders.vdf. Line-based surgery preserves the original
/// file's whitespace and avoids round-tripping through a parser that
/// might reformat the file in ways Steam dislikes.
pub fn strip_app_from_vault_library(
    content: &str,
    vault_drive_letter: &str,
    app_id: &str,
) -> String {
    let vault_letter = vault_drive_letter
        .trim_end_matches(':')
        .to_ascii_uppercase();
    let app_pattern = format!("\"{}\"", app_id);

    let mut out = String::with_capacity(content.len());
    let mut depth: i32 = 0;
    let mut in_target_lib = false;
    let mut in_apps = false;
    let mut lib_depth: Option<i32> = None;
    let mut apps_depth: Option<i32> = None;
    let mut pending_lib = false;
    let mut pending_apps = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_open = trimmed == "{";
        let is_close = trimmed == "}";

        // Drop the app entry line if we're inside the vault's apps block.
        if in_apps
            && in_target_lib
            && trimmed.starts_with(&app_pattern)
            && is_app_entry_line(trimmed, app_id)
        {
            continue;
        }

        out.push_str(line);
        out.push('\n');

        if is_open {
            depth += 1;
            if pending_lib {
                lib_depth = Some(depth);
                in_target_lib = false; // becomes true once we see a matching `path`
                pending_lib = false;
            } else if pending_apps {
                apps_depth = Some(depth);
                in_apps = true;
                pending_apps = false;
            }
        } else if is_close {
            if in_apps && apps_depth == Some(depth) {
                in_apps = false;
                apps_depth = None;
            }
            if lib_depth == Some(depth) {
                in_target_lib = false;
                lib_depth = None;
            }
            depth -= 1;
            pending_apps = false;
            pending_lib = false;
        } else if trimmed.starts_with("\"path\"") {
            if let Some(path_value) = extract_kv_value(trimmed) {
                let first_char = path_value
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_default();
                if first_char == vault_letter {
                    in_target_lib = true;
                }
            }
            pending_apps = false;
            pending_lib = false;
        } else if trimmed == "\"apps\"" {
            pending_apps = true;
            pending_lib = false;
        } else if is_library_index_key(trimmed) {
            pending_lib = true;
            pending_apps = false;
        } else if !trimmed.is_empty() {
            pending_apps = false;
            pending_lib = false;
        }
    }

    out
}

fn is_library_index_key(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('"') || !t.ends_with('"') || t.len() < 3 {
        return false;
    }
    let inner = &t[1..t.len() - 1];
    inner.parse::<u32>().is_ok()
}

fn is_app_entry_line(line: &str, app_id: &str) -> bool {
    let t = line.trim();
    let pat = format!("\"{}\"", app_id);
    if !t.starts_with(&pat) {
        return false;
    }
    let rest = t[pat.len()..].trim_start();
    rest.starts_with('"') && rest.ends_with('"')
}

fn extract_kv_value(line: &str) -> Option<String> {
    let mut parts = line.split('"');
    parts.next()?; // leading whitespace
    parts.next()?; // key
    parts.next()?; // between key and value
    parts.next().map(|s| s.to_string())
}

// ---- Commands ----

/// Prepare the vault for an in-place Steam update of a sibling local
/// install. Performs four destructive-ish steps that are explicitly
/// rolled back by `restore_vault_from_isolation`:
///
/// 1. Rename vault's `appmanifest_<appid>.acf` to `.vaultsync-hidden`
/// 2. Back up `libraryfolders.vdf`
/// 3. Remove the AppID entry from vault's `apps` block in the VDF
/// 4. Gracefully close Steam via `steam://exit` and wait for the
///    process to actually exit (otherwise Steam would clobber our
///    VDF edit on its next config flush)
#[tauri::command]
pub async fn isolate_vault_for_steam_update(
    vault_lib_path: String,
    vault_drive_letter: String,
    app_id: String,
) -> Result<bool, String> {
    // Step 1: hide vault's appmanifest.
    let manifest = vault_manifest_path(&vault_lib_path, &app_id)
        .ok_or_else(|| "Could not derive vault manifest path".to_string())?;
    let hidden = vault_hidden_manifest_path(&vault_lib_path, &app_id)
        .ok_or_else(|| "Could not derive hidden manifest path".to_string())?;
    if hidden.exists() {
        let _ = fs::remove_file(&hidden);
    }
    let manifest_hidden = if manifest.exists() {
        fs::rename(&manifest, &hidden).map_err(|e| {
            format!("Failed to hide vault manifest: {}", e)
        })?;
        true
    } else {
        false
    };

    // Step 2: back up libraryfolders.vdf.
    let vdf = libraryfolders_vdf_path();
    let backup = vdf_backup_path();
    if !vdf.exists() {
        // Restore vault manifest if we can't even find Steam's VDF.
        if manifest_hidden {
            let _ = fs::rename(&hidden, &manifest);
        }
        return Err(format!("libraryfolders.vdf not found at {}", vdf.display()));
    }
    // Remove any stale backup from a previous interrupted run.
    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }
    fs::copy(&vdf, &backup)
        .map_err(|e| format!("Failed to back up libraryfolders.vdf: {}", e))?;

    // Step 3: edit the VDF to remove the AppID from vault's apps block.
    let content = fs::read_to_string(&vdf)
        .map_err(|e| format!("Failed to read libraryfolders.vdf: {}", e))?;
    let stripped = strip_app_from_vault_library(&content, &vault_drive_letter, &app_id);
    fs::write(&vdf, stripped)
        .map_err(|e| format!("Failed to write libraryfolders.vdf: {}", e))?;

    // Step 4: tell Steam to exit, wait for the process.
    if is_steam_running() {
        fire_steam_exit()?;
        if let Err(e) = wait_for_steam_exit(STEAM_EXIT_TIMEOUT_SECS).await {
            // Roll back everything on failure.
            let _ = fs::rename(&backup, &vdf);
            if manifest_hidden {
                let _ = fs::rename(&hidden, &manifest);
            }
            return Err(e);
        }
    }

    Ok(manifest_hidden)
}

/// Undo the modifications from `isolate_vault_for_steam_update`. Safe
/// to call multiple times (idempotent). Called both on the success path
/// (after push-to-vault has written a fresh manifest) and on cancel/
/// error paths.
#[tauri::command]
pub async fn restore_vault_from_isolation(
    vault_lib_path: String,
    app_id: String,
) -> Result<(), String> {
    // Restore libraryfolders.vdf from backup if backup exists.
    let vdf = libraryfolders_vdf_path();
    let backup = vdf_backup_path();
    if backup.exists() {
        // If the real file was modified by Steam in the meantime,
        // overwrite anyway — our backup represents the pre-isolation
        // truth.
        let _ = fs::remove_file(&vdf);
        if let Err(e) = fs::rename(&backup, &vdf) {
            return Err(format!("Failed to restore libraryfolders.vdf: {}", e));
        }
    }

    // Discard any hidden manifest sentinel. On the success path
    // push_to_vault has already written a fresh manifest in its place;
    // on the cancel/error path the original manifest needs restoring.
    let hidden = vault_hidden_manifest_path(&vault_lib_path, &app_id);
    let manifest = vault_manifest_path(&vault_lib_path, &app_id);
    if let (Some(hidden), Some(manifest)) = (hidden, manifest) {
        if hidden.exists() {
            if manifest.exists() {
                // Push-back wrote a fresh manifest — discard the
                // backup since the real one is current.
                let _ = fs::remove_file(&hidden);
            } else {
                // No fresh manifest (cancelled before push) — put the
                // original one back.
                let _ = fs::rename(&hidden, &manifest);
            }
        }
    }

    Ok(())
}

/// One-shot, no-backup edit of libraryfolders.vdf: remove the given
/// AppID from the vault library's apps block. Used by
/// `delete_from_vault` since the deletion is intentionally permanent.
pub fn permanently_strip_app_from_libraryfolders(
    vault_drive_letter: &str,
    app_id: &str,
) -> Result<(), String> {
    let vdf = libraryfolders_vdf_path();
    if !vdf.exists() {
        return Ok(()); // Steam not installed at default path — nothing to do
    }
    let content = fs::read_to_string(&vdf)
        .map_err(|e| format!("Failed to read libraryfolders.vdf: {}", e))?;
    let stripped = strip_app_from_vault_library(&content, vault_drive_letter, app_id);
    if stripped == content {
        return Ok(()); // App wasn't listed — no-op
    }
    fs::write(&vdf, stripped)
        .map_err(|e| format!("Failed to write libraryfolders.vdf: {}", e))
}

// ---- Tests for the line-based stripper ----

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"12345"		"100"
			"67890"		"200"
		}
	}
	"1"
	{
		"path"		"S:\\SteamLibrary"
		"label"		""
		"apps"
		{
			"12345"		"100"
			"99999"		"300"
		}
	}
}
"#;

    #[test]
    fn strips_only_vault_library_entry() {
        let out = strip_app_from_vault_library(SAMPLE, "S", "12345");
        // Vault's 12345 line is gone.
        let vault_apps_start = out.find("\"S:\\\\SteamLibrary\"").unwrap();
        let vault_section = &out[vault_apps_start..];
        assert!(!vault_section.contains("\"12345\"\t\t\"100\""));
        assert!(vault_section.contains("\"99999\"\t\t\"300\""));

        // Local C: library's 12345 line is still present.
        let local_section = &out[..vault_apps_start];
        assert!(local_section.contains("\"12345\"\t\t\"100\""));
        assert!(local_section.contains("\"67890\"\t\t\"200\""));
    }

    #[test]
    fn noop_when_app_not_in_vault() {
        let out = strip_app_from_vault_library(SAMPLE, "S", "0000000");
        assert_eq!(out.trim_end(), SAMPLE.trim_end());
    }

    #[test]
    fn noop_when_vault_letter_missing() {
        let out = strip_app_from_vault_library(SAMPLE, "Z", "12345");
        assert_eq!(out.trim_end(), SAMPLE.trim_end());
    }
}

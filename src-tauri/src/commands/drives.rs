use super::{steam_common_path, vaultsync_db_path, LocalLibrary, SsdInfo};
use rusqlite::Connection;
use std::path::PathBuf;

const DEFAULT_VDF: &str = "C:\\Program Files (x86)\\Steam\\steamapps\\libraryfolders.vdf";

#[tauri::command]
pub async fn scan_vault_ssd(drive_letter: String) -> Result<SsdInfo, String> {
    let letter = drive_letter.trim_end_matches(':');
    let drive_root = PathBuf::from(format!("{}:\\", letter));
    if !drive_root.exists() {
        return Err(format!("Drive {}: is not connected", letter));
    }

    let db_path = vaultsync_db_path(&drive_letter);
    let is_new_db = !db_path.exists();

    {
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open {}: {}", db_path.display(), e))?;
        super::catalog::ensure_schema(&conn).map_err(|e| e.to_string())?;
    }

    if is_new_db {
        // First launch on this SSD: do an initial folder scan if a Steam
        // library is already present at the conventional path.
        let common = steam_common_path(&drive_letter);
        if common.exists() {
            let _ = super::catalog::rescan_ssd(drive_letter.clone()).await;
        }
    }

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    super::catalog::ensure_schema(&conn).map_err(|e| e.to_string())?;

    let id: String = conn
        .query_row(
            "SELECT value FROM ssd_meta WHERE key = 'ssd_uuid'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| {
            let new_id = uuid::Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO ssd_meta (key, value) VALUES ('ssd_uuid', ?1)",
                [&new_id],
            );
            new_id
        });

    let name: String = conn
        .query_row(
            "SELECT value FROM ssd_meta WHERE key = 'ssd_name'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| {
            let default = format!("SSD Vault ({}:)", drive_letter.trim_end_matches(':'));
            let _ = conn.execute(
                "INSERT OR REPLACE INTO ssd_meta (key, value) VALUES ('ssd_name', ?1)",
                [&default],
            );
            default
        });

    let total_games: u32 = conn
        .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
        .unwrap_or(0);

    let _ = conn.execute(
        "INSERT OR REPLACE INTO ssd_meta (key, value) VALUES ('last_scanned', datetime('now'))",
        [],
    );

    Ok(SsdInfo {
        id,
        name,
        drive_letter: drive_letter.trim_end_matches(':').to_string(),
        connected: true,
        total_games,
    })
}

#[tauri::command]
pub async fn scan_local_steam_libraries(
    vault_drive_letter: Option<String>,
) -> Result<Vec<LocalLibrary>, String> {
    let vdf_path = PathBuf::from(DEFAULT_VDF);
    if !vdf_path.exists() {
        return Ok(Vec::new());
    }

    let paths = super::steam::parse_library_folders_vdf(vdf_path.to_string_lossy().to_string())
        .await
        .unwrap_or_default();

    let vault_letter = vault_drive_letter
        .as_deref()
        .map(|s| s.trim_end_matches(':').to_ascii_uppercase());

    let mut libraries = Vec::new();
    for path in paths {
        let common = PathBuf::from(&path).join("steamapps").join("common");
        let drive_letter = path
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();

        // Exclude the SSD vault itself — Steam knows about it as a library
        // since the user formerly used it that way, but for VaultSync it's
        // the source, not a destination.
        if let Some(vault) = vault_letter.as_deref() {
            if drive_letter == vault {
                continue;
            }
        }

        // Skip stale VDF entries: Steam never prunes libraries from
        // libraryfolders.vdf when a drive is disconnected or removed, so
        // the VDF can list drive letters that no longer exist on the
        // machine. Only include libraries that physically exist.
        if !common.exists() {
            continue;
        }

        let mut games = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&common) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        games.push(name.to_string());
                    }
                }
            }
        }

        libraries.push(LocalLibrary {
            path: common.to_string_lossy().to_string(),
            drive_letter,
            games,
        });
    }

    Ok(libraries)
}

#[allow(dead_code)]
pub fn vault_common_path(drive_letter: &str) -> PathBuf {
    steam_common_path(drive_letter)
}

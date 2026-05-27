use super::{steam_common_path, vaultsync_db_path, Game, LocalLibrary, SsdInfo};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;

const DEFAULT_VDF: &str = "C:\\Program Files (x86)\\Steam\\steamapps\\libraryfolders.vdf";

/// Probe drive letters A: through Z: looking for a `vaultsync.db` file
/// at the root. Returns each letter that has one — these are treated
/// as vault SSDs. Cheap call: non-existent drives return immediately
/// on Windows. Safe to run every hotplug tick.
#[tauri::command]
pub async fn discover_vault_letters() -> Result<Vec<String>, String> {
    let mut letters = Vec::new();
    for c in b'A'..=b'Z' {
        let letter = (c as char).to_string();
        let db = vaultsync_db_path(&letter);
        if db.exists() {
            letters.push(letter);
        }
    }
    Ok(letters)
}

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
    vault_drive_letters: Vec<String>,
) -> Result<Vec<LocalLibrary>, String> {
    let vdf_path = PathBuf::from(DEFAULT_VDF);
    if !vdf_path.exists() {
        return Ok(Vec::new());
    }

    let paths = super::steam::parse_library_folders_vdf(vdf_path.to_string_lossy().to_string())
        .await
        .unwrap_or_default();

    let vault_letters: std::collections::HashSet<String> = vault_drive_letters
        .iter()
        .map(|s| s.trim_end_matches(':').to_ascii_uppercase())
        .collect();

    let mut libraries = Vec::new();
    for path in paths {
        let common = PathBuf::from(&path).join("steamapps").join("common");
        let drive_letter = path
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();

        // Exclude vault SSDs — Steam knows about them as libraries since
        // the user formerly used them that way, but for VaultSync each
        // vault is the source, not a destination.
        if vault_letters.contains(&drive_letter) {
            continue;
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

/// Discover Steam games installed in local libraries that are NOT
/// present in any of the user's vault catalogs. Useful for finding
/// games to back up to the vault.
///
/// Caller passes:
///   - `vault_folder_names`: every folder name across all connected
///     vault catalogs (these will be excluded)
///   - `local_library_paths`: list of each local Steam library's
///     `steamapps/common` path (from scan_local_steam_libraries)
///
/// Returns Game-shaped records with sentinel `ssd_id = "__local__"`
/// and `ssd_drive_letter = <drive>` so they fit through the existing
/// frontend pipeline without a parallel type.
#[tauri::command]
pub async fn scan_local_only_games(
    vault_folder_names: Vec<String>,
    local_library_paths: Vec<String>,
) -> Result<Vec<Game>, String> {
    let vault_set: HashSet<String> = vault_folder_names.into_iter().collect();
    let mut out: Vec<Game> = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    for common_path in &local_library_paths {
        let common = PathBuf::from(common_path);
        let steamapps = match common.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let drive_letter = common_path
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();

        // Read every appmanifest in this library; that gives us
        // AppID + canonical title + folder name without re-scanning
        // game folders.
        let entries = match std::fs::read_dir(&steamapps) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let content = match std::fs::read_to_string(&p) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let manifest = match super::catalog::parse_appmanifest(&content) {
                Some(m) => m,
                None => continue,
            };

            // Skip games that are already in a vault.
            if vault_set.contains(&manifest.install_dir) {
                continue;
            }
            // De-dupe across libraries (game installed in two local
            // libraries — unlikely but possible).
            let key = format!("{}:{}", drive_letter, manifest.install_dir);
            if seen_keys.contains(&key) {
                continue;
            }
            seen_keys.insert(key);

            let game_folder = common.join(&manifest.install_dir);
            if !game_folder.exists() {
                continue; // Orphan manifest — game files missing
            }
            let size_gb = super::catalog::folder_size_gb_pub(&game_folder);

            let cover_url = Some(format!(
                "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/library_600x900.jpg",
                manifest.app_id
            ));

            // Use a stable synthetic id so the frontend can key on it.
            // Negative ints would collide with autoincrement; we use
            // a hash of the local path instead.
            let id_seed = format!("{}|{}", common_path, manifest.install_dir);
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&id_seed, &mut hasher);
            let synthetic_id = (std::hash::Hasher::finish(&hasher) as i64).abs();

            out.push(Game {
                id: synthetic_id,
                app_id: Some(manifest.app_id.clone()),
                title: manifest.name,
                folder_name: manifest.install_dir,
                size_gb,
                cover_url,
                build_id: manifest.build_id.clone(),
                local_build_id: manifest.build_id,
                ssd_id: "__local__".to_string(),
                ssd_drive_letter: drive_letter.clone(),
                is_available: true,
                is_installed: true,
                installed_path: Some(common_path.clone()),
                has_update: false,
            });
        }
    }

    Ok(out)
}

use super::{steam_common_path, vaultsync_db_path, Game, GameRecord};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS games (
          id          INTEGER PRIMARY KEY AUTOINCREMENT,
          app_id      TEXT,
          title       TEXT NOT NULL,
          folder_name TEXT NOT NULL UNIQUE,
          size_gb     REAL,
          cover_url   TEXT,
          build_id    TEXT,
          added_at    DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS ssd_meta (
          key   TEXT PRIMARY KEY,
          value TEXT
        );
        ",
    )?;
    // Migration for vaults created by v0.1.0 (no build_id column yet).
    // ALTER TABLE ADD COLUMN errors if the column exists — swallow it.
    let _ = conn.execute("ALTER TABLE games ADD COLUMN build_id TEXT", []);
    Ok(())
}

pub fn open(drive_letter: &str) -> Result<Connection, String> {
    let path = vaultsync_db_path(drive_letter);
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    ensure_schema(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

fn ssd_id(conn: &Connection) -> String {
    conn.query_row(
        "SELECT value FROM ssd_meta WHERE key = 'ssd_uuid'",
        [],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_default()
}

#[tauri::command]
pub async fn get_ssd_catalog(drive_letter: String) -> Result<Vec<Game>, String> {
    let conn = open(&drive_letter)?;
    let id = ssd_id(&conn);
    let letter = drive_letter.trim_end_matches(':').to_string();

    let mut stmt = conn
        .prepare("SELECT id, app_id, title, folder_name, size_gb, cover_url, build_id FROM games ORDER BY title")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Game {
                id: row.get(0)?,
                app_id: row.get(1)?,
                title: row.get(2)?,
                folder_name: row.get(3)?,
                size_gb: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                cover_url: row.get(5)?,
                build_id: row.get(6)?,
                local_build_id: None,
                ssd_id: id.clone(),
                ssd_drive_letter: letter.clone(),
                is_available: true,
                is_installed: false,
                installed_path: None,
                has_update: false,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut games = Vec::new();
    for r in rows {
        games.push(r.map_err(|e| e.to_string())?);
    }
    Ok(games)
}

#[tauri::command]
pub async fn upsert_game(drive_letter: String, game: GameRecord) -> Result<(), String> {
    let conn = open(&drive_letter)?;
    conn.execute(
        "INSERT INTO games (app_id, title, folder_name, size_gb, cover_url)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(folder_name) DO UPDATE SET
           app_id = excluded.app_id,
           title = excluded.title,
           size_gb = excluded.size_gb,
           cover_url = excluded.cover_url",
        params![
            game.app_id,
            game.title,
            game.folder_name,
            game.size_gb,
            game.cover_url,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn rescan_ssd(drive_letter: String) -> Result<Vec<Game>, String> {
    let common = steam_common_path(&drive_letter);
    if !common.exists() {
        return Err(format!(
            "Steam common folder not found at {}",
            common.display()
        ));
    }

    let conn = open(&drive_letter)?;

    let mut on_disk: Vec<(String, f64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&common) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let size_gb = folder_size_gb(&path);
                    on_disk.push((name.to_string(), size_gb));
                }
            }
        }
    }

    let manifests = read_appmanifests(&drive_letter);

    for (folder, size_gb) in &on_disk {
        let manifest = manifests.get(folder.as_str());
        let app_id = manifest.map(|m| m.app_id.clone());
        let title = manifest
            .map(|m| m.name.clone())
            .unwrap_or_else(|| folder.clone());
        let cover_url = manifest.map(|m| {
            format!(
                "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/library_600x900.jpg",
                m.app_id
            )
        });
        let build_id = manifest.and_then(|m| m.build_id.clone());

        conn.execute(
            "INSERT INTO games (app_id, title, folder_name, size_gb, cover_url, build_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(folder_name) DO UPDATE SET
               size_gb = excluded.size_gb,
               app_id = COALESCE(excluded.app_id, games.app_id),
               title = CASE WHEN excluded.app_id IS NOT NULL THEN excluded.title ELSE games.title END,
               cover_url = COALESCE(excluded.cover_url, games.cover_url),
               build_id = COALESCE(excluded.build_id, games.build_id)",
            params![app_id, title, folder, size_gb, cover_url, build_id],
        )
        .map_err(|e| e.to_string())?;
    }

    let disk_names: Vec<String> = on_disk.iter().map(|(n, _)| n.clone()).collect();
    if !disk_names.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(disk_names.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "DELETE FROM games WHERE folder_name NOT IN ({})",
            placeholders
        );
        let params_dyn: Vec<&dyn rusqlite::ToSql> =
            disk_names.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        conn.execute(&sql, params_dyn.as_slice())
            .map_err(|e| e.to_string())?;
    } else {
        conn.execute("DELETE FROM games", []).map_err(|e| e.to_string())?;
    }

    get_ssd_catalog(drive_letter).await
}

pub fn folder_size_gb_pub(path: &Path) -> f64 {
    folder_size_gb(path)
}

fn folder_size_gb(path: &Path) -> f64 {
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    (total as f64) / 1_073_741_824.0
}

#[derive(Debug)]
pub struct AppManifest {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
    pub build_id: Option<String>,
    pub state_flags: Option<String>,
}

/// Read every `appmanifest_*.acf` Steam wrote into the SSD's steamapps
/// folder. These files contain the canonical AppID + display name + the
/// folder name (`installdir`) for each installed game — far more reliable
/// than fuzzy-matching folder names against the public Steam app list.
fn read_appmanifests(drive_letter: &str) -> HashMap<String, AppManifest> {
    let letter = drive_letter.trim_end_matches(':');
    let steamapps = PathBuf::from(format!("{}:\\SteamLibrary\\steamapps", letter));
    let mut map = HashMap::new();

    let entries = match std::fs::read_dir(&steamapps) {
        Ok(e) => e,
        Err(_) => return map,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(m) = parse_appmanifest(&content) {
                map.insert(m.install_dir.clone(), m);
            }
        }
    }
    map
}

pub fn parse_appmanifest(content: &str) -> Option<AppManifest> {
    let mut app_id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut install_dir: Option<String> = None;
    let mut build_id: Option<String> = None;
    let mut state_flags: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"appid\"") {
            app_id = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"name\"") {
            name = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"installdir\"") {
            install_dir = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"buildid\"") {
            build_id = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"StateFlags\"") {
            state_flags = extract_acf_value(trimmed);
        }
    }

    let install_dir = install_dir?;
    Some(AppManifest {
        app_id: app_id?,
        name: name.unwrap_or_else(|| install_dir.clone()),
        install_dir,
        build_id,
        state_flags,
    })
}

/// ACF lines look like: `\t"key"\t\t"value"` — return the second quoted segment.
fn extract_acf_value(line: &str) -> Option<String> {
    let mut parts = line.split('"');
    parts.next()?; // leading whitespace
    parts.next()?; // key
    parts.next()?; // between key and value
    parts.next().map(|s| s.to_string())
}

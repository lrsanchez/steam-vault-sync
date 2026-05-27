use keyvalues_parser::Vdf;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[tauri::command]
pub async fn parse_library_folders_vdf(vdf_path: String) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(&vdf_path)
        .map_err(|e| format!("Failed to read VDF at {}: {}", vdf_path, e))?;
    #[allow(deprecated)]
    let vdf = Vdf::parse(&raw).map_err(|e| format!("VDF parse error: {}", e))?;

    let mut paths = Vec::new();
    let obj = match vdf.value.get_obj() {
        Some(o) => o,
        None => return Ok(paths),
    };

    for (_key, values) in obj.iter() {
        for value in values {
            if let Some(inner) = value.get_obj() {
                if let Some(path_values) = inner.get("path") {
                    for pv in path_values {
                        if let Some(s) = pv.get_str() {
                            paths.push(s.replace("\\\\", "\\"));
                        }
                    }
                }
            }
        }
    }

    Ok(paths)
}

#[tauri::command]
pub async fn register_game_in_steam(app_id: String) -> Result<(), String> {
    open_steam_uri(&format!("steam://install/{}", app_id))
}

#[tauri::command]
pub async fn launch_game(app_id: String) -> Result<(), String> {
    open_steam_uri(&format!("steam://rungameid/{}", app_id))
}

fn open_steam_uri(uri: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", uri])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = uri;
        Err("Steam URIs: Windows only".to_string())
    }
}

#[tauri::command]
pub async fn check_installed_games(
    vault_games: Vec<String>,
    local_libraries: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for lib in &local_libraries {
        let common = PathBuf::from(lib);
        for game_folder in &vault_games {
            let candidate = common.join(game_folder);
            if candidate.exists() && candidate.is_dir() {
                map.insert(game_folder.clone(), lib.clone());
            }
        }
    }
    Ok(map)
}

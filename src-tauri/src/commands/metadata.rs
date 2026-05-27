use super::GameMetadata;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

static APP_LIST_CACHE: Lazy<Mutex<Option<Vec<SteamApp>>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, Deserialize)]
struct SteamApp {
    appid: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct AppListResponse {
    applist: AppListInner,
}

#[derive(Debug, Deserialize)]
struct AppListInner {
    apps: Vec<SteamApp>,
}

#[derive(Debug, Deserialize)]
struct AppDetailsWrapper {
    success: bool,
    data: Option<AppDetailsData>,
}

#[derive(Debug, Deserialize)]
struct AppDetailsData {
    name: String,
    steam_appid: u64,
    header_image: Option<String>,
    short_description: Option<String>,
}

#[tauri::command]
pub async fn fetch_steam_metadata(
    app_id: String,
    api_key: String,
) -> Result<GameMetadata, String> {
    let _ = api_key;
    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}",
        app_id
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Steam API request failed: {}", e))?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Steam API parse error: {}", e))?;

    let entry = json.get(&app_id).ok_or_else(|| "App not found".to_string())?;
    let wrapper: AppDetailsWrapper = serde_json::from_value(entry.clone())
        .map_err(|e| format!("Steam API shape error: {}", e))?;

    if !wrapper.success {
        return Err(format!("Steam API returned no data for app {}", app_id));
    }

    let data = wrapper.data.ok_or_else(|| "No data field".to_string())?;
    let library_cover = Some(format!(
        "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/library_600x900.jpg",
        data.steam_appid
    ));

    Ok(GameMetadata {
        app_id: data.steam_appid.to_string(),
        name: data.name,
        header_image: data.header_image,
        library_cover,
        short_description: data.short_description,
    })
}

#[tauri::command]
pub async fn resolve_app_id(folder_name: String) -> Result<Option<String>, String> {
    let apps = get_app_list().await?;
    let target = normalize(&folder_name);

    let mut best: Option<(u64, usize)> = None;
    for app in apps.iter() {
        let norm = normalize(&app.name);
        if norm == target {
            return Ok(Some(app.appid.to_string()));
        }
        if norm.contains(&target) || target.contains(&norm) {
            let score = norm.len().abs_diff(target.len());
            if best.map(|(_, s)| score < s).unwrap_or(true) {
                best = Some((app.appid, score));
            }
        }
    }

    Ok(best.map(|(id, _)| id.to_string()))
}

async fn get_app_list() -> Result<Vec<SteamApp>, String> {
    {
        let cache = APP_LIST_CACHE.lock().unwrap();
        if let Some(apps) = cache.as_ref() {
            return Ok(apps.clone());
        }
    }

    let url = "https://api.steampowered.com/ISteamApps/GetAppList/v2/";
    let resp: AppListResponse = reqwest::get(url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let apps = resp.applist.apps;
    *APP_LIST_CACHE.lock().unwrap() = Some(apps.clone());
    Ok(apps)
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

// ---- Vault update detection ----

/// Check which AppIDs have a pending Steam update by scanning the
/// `appmanifest_*.acf` files across all Steam libraries the user has
/// (including the vault, which Steam knows about via libraryfolders.vdf).
///
/// Steam writes pending-update state directly into the appmanifest's
/// `StateFlags` field — the same data its own UI uses to render "Update
/// queued / Unscheduled update / Validating" labels. A value of 4 means
/// "Fully Installed". Anything else means something is pending:
///   - 2 = Uninstalled
///   - 8 = Update Required
///   - 16 = Files Missing
///   - 32 = Update Running
///   - 64 = Files Corrupt
///   - 1024+ = Update Queued / Unscheduled
///   - (Steam combines flags bitwise, so values like 1026 are common)
///
/// Library paths passed in should point to each Steam library's
/// `steamapps/common` folder; the manifests live in the parent
/// `steamapps/`.
#[tauri::command]
pub async fn check_vault_updates(
    library_paths: Vec<String>,
    app_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    use std::collections::HashSet;
    let mut outdated: HashSet<String> = HashSet::new();

    let steamapps_dirs: Vec<PathBuf> = library_paths
        .iter()
        .filter_map(|p| PathBuf::from(p).parent().map(|x| x.to_path_buf()))
        .collect();

    for app_id in &app_ids {
        for dir in &steamapps_dirs {
            let manifest = dir.join(format!("appmanifest_{}.acf", app_id));
            if !manifest.exists() {
                continue;
            }
            let content = match std::fs::read_to_string(&manifest) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Some(parsed) = super::catalog::parse_appmanifest(&content) {
                let fully_installed = parsed
                    .state_flags
                    .as_deref()
                    .map(|s| s == "4")
                    .unwrap_or(false);
                if !fully_installed {
                    outdated.insert(app_id.clone());
                    break;
                }
            }
        }
    }

    Ok(outdated.into_iter().collect())
}

// ---- Polling Steam's progress on a local appmanifest ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalManifestState {
    pub app_id: String,
    pub build_id: Option<String>,
    pub state_flags: Option<String>,
    /// Convenience flag derived from state_flags: true when Steam reports
    /// "Fully Installed" (StateFlags = 4). Used by the auto-update
    /// workflow to know when Steam has finished its patch.
    pub fully_installed: bool,
}

/// Read the local Steam library's appmanifest for the given AppID.
/// Returns `None` if the manifest is missing (game not installed in
/// that library). Used by the auto-update workflow to poll while Steam
/// patches the local copy.
#[tauri::command]
pub async fn read_local_appmanifest_state(
    library_path: String,
    app_id: String,
) -> Result<Option<LocalManifestState>, String> {
    // library_path is the steamapps/common folder; the manifest is in
    // the parent steamapps/ folder.
    let common = PathBuf::from(&library_path);
    let steamapps = match common.parent() {
        Some(p) => p.to_path_buf(),
        None => return Ok(None),
    };
    let manifest = steamapps.join(format!("appmanifest_{}.acf", app_id));
    if !manifest.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    let parsed = match super::catalog::parse_appmanifest(&content) {
        Some(p) => p,
        None => return Ok(None),
    };
    let fully_installed = parsed
        .state_flags
        .as_deref()
        .map(|s| s == "4")
        .unwrap_or(false);
    Ok(Some(LocalManifestState {
        app_id: parsed.app_id,
        build_id: parsed.build_id,
        state_flags: parsed.state_flags,
        fully_installed,
    }))
}

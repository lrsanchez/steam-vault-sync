use super::GameMetadata;
use once_cell::sync::Lazy;
use serde::Deserialize;
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

pub mod catalog;
pub mod copy;
pub mod drives;
pub mod metadata;
pub mod steam;
pub mod vdf_isolation;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: i64,
    pub app_id: Option<String>,
    pub title: String,
    pub folder_name: String,
    pub size_gb: f64,
    pub cover_url: Option<String>,
    pub build_id: Option<String>,
    pub local_build_id: Option<String>,
    pub ssd_id: String,
    pub ssd_drive_letter: String,
    pub is_available: bool,
    pub is_installed: bool,
    pub installed_path: Option<String>,
    pub has_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRecord {
    pub app_id: Option<String>,
    pub title: String,
    pub folder_name: String,
    pub size_gb: f64,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLibrary {
    pub path: String,
    pub drive_letter: String,
    pub games: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsdInfo {
    pub id: String,
    pub name: String,
    pub drive_letter: String,
    pub connected: bool,
    pub total_games: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadata {
    pub app_id: String,
    pub name: String,
    pub header_image: Option<String>,
    pub library_cover: Option<String>,
    pub short_description: Option<String>,
}

pub fn steam_common_path(drive_letter: &str) -> std::path::PathBuf {
    let letter = drive_letter.trim_end_matches(':');
    std::path::PathBuf::from(format!("{}:\\SteamLibrary\\steamapps\\common", letter))
}

pub fn vaultsync_db_path(drive_letter: &str) -> std::path::PathBuf {
    let letter = drive_letter.trim_end_matches(':');
    std::path::PathBuf::from(format!("{}:\\vaultsync.db", letter))
}

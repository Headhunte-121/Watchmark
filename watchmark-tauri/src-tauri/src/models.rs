// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw println! allowed.

use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_to_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrFloat {
        Int(i32),
        Float(f64),
    }

    match IntOrFloat::deserialize(deserializer) {
        Ok(IntOrFloat::Int(i)) => Ok(i),
        Ok(IntOrFloat::Float(f)) => Ok(f.round() as i32),
        Err(e) => Err(e),
    }
}

pub fn deserialize_flexible_runtime<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawRuntime {
        Int(i32),
        Float(f64),
        String(String),
        Array(Vec<i32>),
        Null,
    }

    match RawRuntime::deserialize(deserializer) {
        Ok(RawRuntime::Int(i)) => Ok(i),
        Ok(RawRuntime::Float(f)) => Ok(f.round() as i32),
        Ok(RawRuntime::String(s)) => {
            if s.trim().is_empty() || s.to_lowercase() == "n/a" {
                Ok(0)
            } else {
                s.parse::<i32>()
                    .or_else(|_| s.parse::<f64>().map(|f| f.round() as i32))
                    .or(Ok(0))
            }
        }
        Ok(RawRuntime::Array(arr)) => {
            if !arr.is_empty() {
                Ok(arr[0])
            } else {
                Ok(0)
            }
        }
        Ok(RawRuntime::Null) | Err(_) => Ok(0),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TmdbEpisode {
    pub season_number: u32,
    pub episode_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_runtime")]
    pub runtime: i32,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_flexible_runtime() {
        let json_int = json!({
            "season_number": 1,
            "episode_number": 1,
            "runtime": 45
        });
        let ep1: TmdbEpisode = serde_json::from_value(json_int).unwrap();
        assert_eq!(ep1.runtime, 45);

        let json_str_int = json!({
            "season_number": 1,
            "episode_number": 1,
            "runtime": "45"
        });
        let ep2: TmdbEpisode = serde_json::from_value(json_str_int).unwrap();
        assert_eq!(ep2.runtime, 45);

        let json_na = json!({
            "season_number": 1,
            "episode_number": 1,
            "runtime": "N/A"
        });
        let ep3: TmdbEpisode = serde_json::from_value(json_na).unwrap();
        assert_eq!(ep3.runtime, 0);

        let json_null = json!({
            "season_number": 1,
            "episode_number": 1,
            "runtime": null
        });
        let ep4: TmdbEpisode = serde_json::from_value(json_null).unwrap();
        assert_eq!(ep4.runtime, 0);

        let json_missing = json!({
            "season_number": 1,
            "episode_number": 1
        });
        let ep5: TmdbEpisode = serde_json::from_value(json_missing).unwrap();
        assert_eq!(ep5.runtime, 0);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MediaType {
    TV,
    Movie,
    Unknown,
}

impl MediaType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "TV" => MediaType::TV,
            "Movie" => MediaType::Movie,
            _ => MediaType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::TV => "TV",
            MediaType::Movie => "Movie",
            MediaType::Unknown => "Unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Media {
    pub id: i32,
    pub tmdb_id: String,
    pub r#type: String,
    pub title: String,
    pub synopsis: String,
    pub poster_path: String,
    pub backdrop_path: String,
    pub total_episodes: i32,
    pub total_available: Option<i32>,
    pub status: String,
    pub vote_average: f64,
    pub user_rating: Option<i32>,
    pub release_date: String,
    pub is_exact_date: bool,
    #[serde(default)]
    pub is_date_known: bool,
    #[serde(default)]
    pub backdrop_fallback: Option<String>,
    #[serde(default)]
    pub genres: String,
    #[serde(default)]
    pub networks: String,
    pub collection_id: Option<i32>,
    pub collection_name: Option<String>,

    #[serde(default)]
    pub collection_parts: Option<serde_json::Value>,

    // Virtual fields
    #[serde(default)]
    pub is_unaired: bool,
    #[serde(default)]
    pub completed_eps: i32,
    #[serde(default)]
    pub last_watched: String,
    #[serde(default)]
    pub min_year: String,
    #[serde(default)]
    pub max_year: String,
    #[serde(default)]
    pub seasons: Vec<u32>,
    #[serde(default)]
    pub episodes: Vec<Episode>,
    /// Season number (2 or later) that premiered within the last
    /// `NEW_SEASON_WINDOW_DAYS` days, if any. Drives the "New Season" badge.
    #[serde(default)]
    pub new_season: Option<u32>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Episode {
    pub id: i32,
    pub media_id: i32,
    pub season_num: u32,
    pub ep_num: u32,
    pub title: String,
    pub runtime: i32,
    pub still_path: String,
    pub watch_count: i32,
    pub last_position: u32,
    pub status: String,
    #[serde(default)]
    pub file_path: String,
    pub overview: String,
    #[serde(default)]
    pub season_overview: String,
    pub air_date: String,
    pub is_exact_date: bool,
    #[serde(default)]
    pub is_date_known: bool,
    #[serde(default)]
    pub progress_percentage: f64,

    // Virtual fields joined
    #[serde(default)]
    pub is_unaired: bool,
    #[serde(default)]
    pub show_title: String,
    #[serde(default)]
    pub backdrop_path: String,
    #[serde(default)]
    pub backdrop_fallback: Option<String>,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub poster_path: String,
    #[serde(default)]
    pub is_fallback_image: bool,
    #[serde(default)]
    pub potential_spoiler: bool,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct HistoryEntry {
    pub hist_id: i32,
    pub timestamp: i64,
    #[serde(default)]
    pub session_id: String,
    pub is_legacy: i32,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub end_time: String,
    pub pause_count: i32,
    pub completion_ratio: f64,

    // Joined episode data
    pub episode_id: i32,
    pub season_num: u32,
    pub ep_num: u32,
    pub ep_title: String,
    pub still_path: String,
    pub air_date: String,
    pub is_exact_date: bool,
    #[serde(default)]
    pub is_date_known: bool,
    #[serde(default)]
    pub runtime: i32,
    #[serde(default)]
    pub progress_percentage: f64,

    // Joined media data
    pub media_id: i32,
    pub show_title: String,
    pub poster_path: String,
    pub backdrop_path: String,
    pub media_type: String,
    #[serde(default)]
    pub time_capsule: Option<serde_json::Value>,
    #[serde(default)]
    pub is_fallback_image: bool,
    #[serde(default)]
    pub potential_spoiler: bool,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[allow(dead_code)]
pub struct Stats {
    pub eps_watched: i32,
    pub hrs_watched: i32,
    pub shows_completed: i32,
    pub avg_rating: f64,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[allow(dead_code)]
pub struct DashboardData {
    pub hero_ep: Option<Episode>,
    #[serde(default)]
    pub cw_eps: Vec<Episode>,
    #[serde(default)]
    pub recent_media: Vec<Media>,
    pub stats: Stats,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UnmatchedFile {
    pub file_path: String,
    pub filename: String,
    pub parsed_series: Option<String>,
    pub parsed_season: Option<u32>,
    pub parsed_episode: Option<u32>,
    pub group_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub vlc_path: String,
    #[serde(deserialize_with = "deserialize_to_i32", default = "default_width")]
    pub width: i32,
    #[serde(deserialize_with = "deserialize_to_i32", default = "default_height")]
    pub height: i32,
    #[serde(deserialize_with = "deserialize_to_i32", default = "default_x")]
    pub x: i32,
    #[serde(deserialize_with = "deserialize_to_i32", default = "default_y")]
    pub y: i32,
    pub tmdb_api_key: String,
    pub cinema_mode: bool,

    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_auto_complete_threshold")]
    pub auto_complete_threshold: i32,
    #[serde(default = "default_binge_grouping_hours")]
    pub binge_grouping_hours: i32,
    #[serde(default = "default_auto_resume")]
    pub auto_resume: bool,
    #[serde(default = "default_auto_scan_on_boot")]
    pub auto_scan_on_boot: bool,
    #[serde(default = "default_logging_level")]
    pub global_log_level: String,
    #[serde(default = "default_module_logs")]
    pub module_logs: std::collections::HashMap<String, String>,

    #[serde(default = "default_false")]
    pub is_fullscreen: bool,
    #[serde(default = "default_false")]
    pub is_maximized: bool,
    #[serde(default = "default_last_backup_timestamp")]
    pub last_backup_timestamp: i64,
    #[serde(default = "default_last_backup_status")]
    pub last_backup_status: String,
    #[serde(default = "default_last_backup_error")]
    pub last_backup_error: String,
    #[serde(default = "default_high_performance_mode")]
    pub high_performance_mode: bool,
    #[serde(default = "default_last_backup_timestamp")]
    pub last_auto_sync_timestamp: i64,
    #[serde(default = "default_empty_string")]
    pub last_scanned_path: String,
    #[serde(default = "default_video_extensions")]
    pub supported_extensions: Vec<String>,
    #[serde(default = "default_backup_retention_count")]
    pub backup_retention_count: usize,
}

fn default_video_extensions() -> Vec<String> {
    vec![
        "mp4".to_string(),
        "mkv".to_string(),
        "avi".to_string(),
        "mov".to_string(),
        "wmv".to_string(),
        "flv".to_string(),
        "webm".to_string(),
    ]
}

fn default_empty_string() -> String {
    "".to_string()
}

fn default_width() -> i32 {
    1280
}
fn default_height() -> i32 {
    800
}
fn default_x() -> i32 {
    100
}
fn default_y() -> i32 {
    100
}
fn default_false() -> bool {
    false
}
fn default_language() -> String {
    "en-US".to_string()
}
fn default_auto_complete_threshold() -> i32 {
    90
}
fn default_binge_grouping_hours() -> i32 {
    6
}
fn default_auto_resume() -> bool {
    true
}
fn default_auto_scan_on_boot() -> bool {
    false
}
fn default_logging_level() -> String {
    "Info".to_string()
}
fn default_module_logs() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for module in crate::logging::CORE_MODULES {
        map.insert(module.to_string(), "info".to_string());
    }
    map
}
fn default_last_backup_timestamp() -> i64 {
    0
}
fn default_last_backup_status() -> String {
    "".to_string()
}
fn default_last_backup_error() -> String {
    "".to_string()
}
fn default_high_performance_mode() -> bool {
    false
}
fn default_backup_retention_count() -> usize {
    5
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vlc_path: String::new(),
            width: default_width(),
            height: default_height(),
            x: default_x(),
            y: default_y(),
            tmdb_api_key: String::new(),
            cinema_mode: true,
            language: default_language(),
            auto_complete_threshold: default_auto_complete_threshold(),
            binge_grouping_hours: default_binge_grouping_hours(),
            auto_resume: default_auto_resume(),
            auto_scan_on_boot: default_auto_scan_on_boot(),
            global_log_level: default_logging_level(),
            module_logs: default_module_logs(),
            last_backup_timestamp: default_last_backup_timestamp(),
            last_backup_status: default_last_backup_status(),
            last_backup_error: default_last_backup_error(),
            high_performance_mode: default_high_performance_mode(),
            last_auto_sync_timestamp: default_last_backup_timestamp(),
            is_fullscreen: default_false(),
            is_maximized: default_false(),
            last_scanned_path: default_empty_string(),
            supported_extensions: default_video_extensions(),
            backup_retention_count: default_backup_retention_count(),
        }
    }
}

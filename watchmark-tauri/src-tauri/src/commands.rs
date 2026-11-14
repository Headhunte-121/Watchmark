// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw tracing::debug! allowed.

/* WATCHMARK STANDARD PATTERN: All asynchronous data commands MUST implement requestId for cancellation support and pagination (page/limit) for UI performance. Follow this signature for all future connections to maintain Phase 1 & 2 integrity. */

use crate::db::get_db_connection;
use crate::error::{handle_panic, AppError};
use crate::models::{Media, Settings, UnmatchedFile};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tauri::{Emitter, Manager};

fn calculate_gap(air_date_str: &str, watch_ts: i64) -> Option<serde_json::Value> {
    if air_date_str.is_empty() {
        return None;
    }

    let air_date = match NaiveDate::parse_from_str(air_date_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return None,
    };

    let air_time = air_date.and_hms_opt(0, 0, 0)?;

    let watch_time_local = match Local.timestamp_opt(watch_ts, 0).single() {
        Some(dt) => dt.naive_local(),
        None => return None,
    };

    let is_early = watch_time_local < air_time;
    let (start, end) = if is_early {
        (watch_time_local, air_time)
    } else {
        (air_time, watch_time_local)
    };

    let mut years = end.year() - start.year();
    let mut months = end.month() as i32 - start.month() as i32;
    let mut days = end.day() as i32 - start.day() as i32;

    if end.time() < start.time() {
        days -= 1;
    }

    if days < 0 {
        months -= 1;
        let prev_month = if end.month() == 1 {
            12
        } else {
            end.month() - 1
        };
        let prev_month_year = if end.month() == 1 {
            end.year() - 1
        } else {
            end.year()
        };
        let next_month = if prev_month == 12 { 1 } else { prev_month + 1 };
        let next_year = if prev_month == 12 {
            prev_month_year + 1
        } else {
            prev_month_year
        };
        let d1 = NaiveDate::from_ymd_opt(prev_month_year, prev_month, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
        days += (d2 - d1).num_days() as i32;
    }

    if months < 0 {
        years -= 1;
        months += 12;
    }

    let total_days = (end.date() - start.date()).num_days();

    Some(serde_json::json!({
        "is_early": is_early,
        "years": years,
        "months": months,
        "days": days,
        "total_days": total_days
    }))
}
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tokio::sync::mpsc;

#[cfg(test)]
mod tests_feature_5_8 {
    use super::*;
    use chrono::{Local, NaiveDate, TimeZone};

    #[test]
    fn test_time_capsule_decade_delay() {
        // Air date 1960-01-01
        // Watched 2024-01-01
        let air_date = "1960-01-01";
        let watch_ts = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp();

        let gap = calculate_gap(air_date, watch_ts).unwrap();
        assert_eq!(gap["years"], 64);
        assert_eq!(gap["is_early"], false);
    }

    #[test]
    fn test_time_capsule_same_day() {
        // Watched on Release Day
        let air_date = "2024-01-01";
        // Let's say we watched it at 2 PM local time on that day
        let watch_ts = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(14, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp();

        let gap = calculate_gap(air_date, watch_ts).unwrap();
        assert_eq!(gap["total_days"], 0);
        assert_eq!(gap["is_early"], false);
    }

    #[test]
    fn test_time_capsule_early_watch() {
        // Time Traveler Test / Leak
        let air_date = "2025-01-01";
        let watch_ts = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp();

        let gap = calculate_gap(air_date, watch_ts).unwrap();
        assert_eq!(gap["is_early"], true);
    }

    #[test]
    fn test_time_capsule_leap_year() {
        // Aired Feb 29, 2024. Watched Feb 28, 2025. Exactly 1 non-leap year (365 days).
        let air_date = "2024-02-29";
        let watch_ts = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2025, 2, 28)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp();

        let gap = calculate_gap(air_date, watch_ts).unwrap();
        assert_eq!(gap["total_days"], 365);
    }

    #[test]
    fn test_time_capsule_null_date() {
        // NULL air date
        let gap = calculate_gap("", 1234567890);
        assert!(gap.is_none());
    }

    #[test]
    fn test_schedule_shift_tbd() {
        let air_date = "TBD";
        // Should parse as error and be handled natively before calling calculate_gap,
        // but if calculate_gap receives it, it should return None cleanly.
        let gap = calculate_gap(air_date, 1234567890);
        assert!(gap.is_none());
    }

    #[test]
    fn test_time_traveler_backdate_gap() {
        // Simulating backdate from the prompt. We test calculate_gap with an early backdated timestamp.
        let air_date = "2024-10-01"; // show aired in 2024
        let watch_ts = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2010, 6, 1)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp();

        let gap = calculate_gap(air_date, watch_ts).unwrap();
        assert_eq!(gap["is_early"], true);
        assert!(gap["years"].as_i64().unwrap() >= 14);
    }
}

use crate::task_queue::DbTaskQueue;

use tokio_util::sync::CancellationToken;

pub async fn check_rate_limit(state: &tauri::State<'_, AppState>) {
    while state.is_rate_limited.load(Ordering::SeqCst) {
        let reset = state.rate_limit_reset.load(Ordering::SeqCst);
        let now = chrono::Utc::now().timestamp();
        if now >= reset {
            state.is_rate_limited.store(false, Ordering::SeqCst);
            break;
        } else {
            let sleep_dur = (reset - now) as u64;
            tokio::time::sleep(tokio::time::Duration::from_secs(sleep_dur.max(1))).await;
        }
    }
}

pub struct AppState {
    pub settings: Arc<RwLock<Settings>>,
    pub settings_tx: mpsc::Sender<Settings>,
    pub db_queue: Arc<DbTaskQueue>,
    pub is_maintenance_mode: AtomicBool,
    pub is_api_authorized: AtomicBool,
    pub is_rate_limited: AtomicBool,
    pub rate_limit_reset: AtomicI64,
    pub stats_cache: Arc<RwLock<Option<Value>>>,
    pub read_semaphore: Arc<tokio::sync::Semaphore>,
    pub cancel_tokens: Arc<RwLock<HashMap<String, CancellationToken>>>,
    pub failed_image_syncs: Arc<RwLock<std::collections::HashSet<(String, String)>>>,
    pub is_scan_cancelled: Arc<AtomicBool>,
    pub is_scan_paused: Arc<AtomicBool>,
    pub live_playback_time: Arc<RwLock<f64>>,
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn cancel_task(request_id: String, state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    let mut tokens = state.cancel_tokens.write().unwrap();
    if let Some(token) = tokens.remove(&request_id) {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn optimize_database(state: tauri::State<'_, AppState>) -> Result<(u64, u64), AppError> {
    if state
        .is_maintenance_mode
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::Custom(
            "System Busy: Maintenance mode is already active.".to_string(),
        ));
    }

    let db_path = crate::db::get_db_path();

    // Size Calculation Before
    let old_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    // Disk Space Check: VACUUM essentially creates a copy of the database. Check if there's enough free space.
    // Ensure we check the parent directory of the db
    if let Some(parent) = db_path.parent() {
        if let Ok(space) = fs3::available_space(parent) {
            if space < old_size {
                state.is_maintenance_mode.store(false, Ordering::SeqCst);
                return Err(AppError::Custom(
                    "Optimization failed: Not enough disk space to re-index.".to_string(),
                ));
            }
        }
    }

    let db_queue = state.db_queue.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    db_queue.push_high_priority(move |conn| {
        // Run VACUUM and ANALYZE
        let result = (|| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "VACUUM;
                 ANALYZE;",
            )?;
            Ok(())
        })();

        let _ = tx.send(result);
    });

    let res = rx.await;
    state.is_maintenance_mode.store(false, Ordering::SeqCst);

    match res {
        Ok(Ok(_)) => {
            // Size Calculation After
            let new_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
            let saved_bytes = old_size.saturating_sub(new_size);

            let percentage = if old_size > 0 {
                ((saved_bytes as f64 / old_size as f64) * 100.0).round() as u64
            } else {
                0
            };

            tracing::info!(
                "[DB] ✨ Database optimization complete. Reclaimed {} bytes ({}%).",
                saved_bytes,
                percentage
            );
            Ok((saved_bytes, percentage))
        }
        Ok(Err(e)) => Err(AppError::Custom(format!(
            "Database optimization failed: {}",
            e
        ))),
        Err(_) => Err(AppError::Custom("Database worker dropped task".to_string())),
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn get_live_playback_time(state: tauri::State<'_, AppState>) -> Result<f64, AppError> {
    let time = state.live_playback_time.read().unwrap();
    Ok(*time)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, AppError> {
    let settings = state.settings.read().unwrap();
    Ok(settings.clone())
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn repair_paths(old_root: String, new_root: String) -> Result<i32, AppError> {
    handle_panic(|| {
        let conn = get_db_connection()?;
        let affected = conn.execute(
            "UPDATE Local_Files SET file_path = REPLACE(file_path, ?, ?) WHERE file_path LIKE ?",
            params![old_root, new_root, format!("{}%", old_root)],
        )?;
        Ok(affected as i32)
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn remove_local_link(episode_id: i32) -> Result<(), AppError> {
    handle_panic(|| {
        let conn = get_db_connection()?;
        conn.execute(
            "DELETE FROM Local_Files WHERE episode_id = ?",
            params![episode_id],
        )?;
        Ok(())
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn ignore_unmatched_group(
    group_key: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    // We execute the delete in the background queue so the UI can respond immediately
    let db_queue = state.db_queue.clone();

    db_queue.push_high_priority(move |conn| {
        let _ = conn.execute(
            "DELETE FROM Unmatched_Files WHERE group_key = ?",
            params![group_key],
        );
    });

    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn validate_and_hash_file(episode_id: i32, file_path: String) -> Result<Value, AppError> {
    let task = tokio::task::spawn_blocking(move || {
        handle_panic(|| {
            let path = std::path::PathBuf::from(&file_path);

            if !path.exists() {
                return Ok(json!({ "status": "missing", "path": file_path }));
            }

            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if file_size == 0 {
                return Ok(json!({ "status": "corrupted", "path": file_path }));
            }

            // Calculate sparse hash
            match crate::hash::compute_sparse_hash(&path) {
                Ok(hash) => {
                    if let Ok(conn) = get_db_connection() {
                        let _ = conn.execute(
                            "UPDATE Local_Files SET file_hash = ? WHERE episode_id = ?",
                            params![hash, episode_id],
                        );
                    }
                    Ok(json!({ "status": "ok", "hash": hash }))
                }
                Err(_) => {
                    Ok(json!({ "status": "error", "message": "Failed to read file for hashing." }))
                }
            }
        })
    });

    task.await
        .unwrap_or(Err(AppError::Custom("Task panicked".to_string())))
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn update_local_file(episode_id: i32, new_path: String) -> Result<(), AppError> {
    handle_panic(|| {
        let conn = get_db_connection()?;
        let file_size = std::fs::metadata(&new_path).map(|m| m.len()).unwrap_or(0) as i64;
        conn.execute(
            "INSERT INTO Local_Files (episode_id, file_path, file_size) VALUES (?, ?, ?) ON CONFLICT(episode_id) DO UPDATE SET file_path=excluded.file_path, file_size=excluded.file_size",
            params![episode_id, new_path, file_size],
        )?;
        Ok(())
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn validate_tmdb_key(
    key: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    // Sanitize the key
    let sanitized_key = key
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();

    if sanitized_key.is_empty() {
        return Ok(serde_json::json!({
            "success": false,
            "sanitized_key": sanitized_key,
            "error_msg": "API key cannot be empty."
        }));
    }

    match crate::tmdb::validate_key(&sanitized_key).await {
        Ok(true) => {
            state.is_api_authorized.store(true, Ordering::SeqCst);
            Ok(serde_json::json!({
                "success": true,
                "sanitized_key": sanitized_key,
                "error_msg": None::<String>
            }))
        }
        Ok(false) => {
            state.is_api_authorized.store(false, Ordering::SeqCst);
            Ok(serde_json::json!({
                "success": false,
                "sanitized_key": sanitized_key,
                "error_msg": "Invalid API Key"
            }))
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.starts_with("RATE_LIMIT:") {
                let parts: Vec<&str> = err_str.split(':').collect();
                let retry_after = parts.get(1).unwrap_or(&"1").parse::<u64>().unwrap_or(1);

                return Ok(serde_json::json!({
                    "success": false,
                    "sanitized_key": sanitized_key,
                    "error_msg": format!("RATE_LIMIT:{}", retry_after)
                }));
            }

            Ok(serde_json::json!({
                "success": false,
                "sanitized_key": sanitized_key,
                "error_msg": format!("Network error: {}", err_str)
            }))
        }
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(settings, state))]
pub async fn save_settings(
    settings: Settings,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    {
        let mut current_settings = state.settings.write().unwrap();
        *current_settings = settings.clone();
    }

    if let Err(e) = crate::logging::set_levels(&settings.global_log_level, &settings.module_logs) {
        tracing::error!(action = "save_settings", error = %e, "[APP] 🚨 Failed to apply dynamic tracing filter.");
    }

    // Send to debouncer task
    state
        .settings_tx
        .send(settings)
        .await
        .map_err(|e| AppError::Custom(e.to_string()))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn update_log_settings(
    global_level: String,
    module_settings: HashMap<String, String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let mut updated_settings = {
        let cache = state.settings.read().unwrap();
        cache.clone()
    };

    updated_settings.global_log_level = global_level.clone();
    updated_settings.module_logs = module_settings.clone();

    {
        let mut cache = state.settings.write().unwrap();
        *cache = updated_settings.clone();
    }

    if let Err(e) = crate::logging::set_levels(&global_level, &module_settings) {
        tracing::error!(action = "update_log_settings", error = %e, "[APP] 🚨 Failed to apply dynamic tracing filter.");
        return Err(AppError::Custom(e));
    }

    if let Err(e) = state.settings_tx.send(updated_settings.clone()).await {
        return Err(AppError::Custom(format!(
            "Failed to queue settings save: {}",
            e
        )));
    }

    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn get_available_modules() -> Result<HashMap<String, String>, AppError> {
    let mut map = HashMap::new();

    if let Ok(settings) = crate::settings::load_settings() {
        map = settings.module_logs;
    }

    // Ensure core modules exist
    for module in crate::logging::CORE_MODULES {
        map.entry(module.to_string())
            .or_insert_with(|| "info".to_string());
    }

    Ok(map)
}

#[tauri::command]
pub fn get_app_data_dir() -> Result<String, AppError> {
    Ok(crate::db::get_app_data_dir().to_string_lossy().to_string())
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn get_media_history_count(media_id: i32) -> Result<i32, AppError> {
    handle_panic(|| {
        let conn = crate::db::get_readonly_connection()?;
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM History WHERE episode_id IN (SELECT id FROM Episodes WHERE media_id = ?)",
            rusqlite::params![media_id],
            |row| row.get(0),
        ).unwrap_or(0);
        Ok(count)
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub fn delete_media_cmd(
    media_id: i32,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let db_queue = state.db_queue.clone();

    // Cancel any active fetch loops for this media item
    {
        let mut tokens = state.cancel_tokens.write().unwrap();
        if let Some(token) = tokens.remove(&media_id.to_string()) {
            token.cancel();
            tracing::info!(
                "[IPC: CANCEL] 🛑 Cancelled background fetch for media_id {}",
                media_id
            );
        }
    }

    if let Ok(mut cache) = state.stats_cache.write() {
        *cache = None;
    }

    // Instead of synchronous delete, we push it to the low priority queue
    db_queue.push_low_priority(move |conn| {
        // We perform the deletion inside the background worker thread.
        let tx = match conn.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                let _ = app.emit(
                    "media-delete-failed",
                    json!({ "media_id": media_id, "error": e.to_string() }),
                );
                return;
            }
        };

        // Query episodes to delete their cached still images
        let mut ep_ids: Vec<i32> = Vec::new();
        if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id = ?") {
            if let Ok(mut rows) = stmt.query([media_id]) {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(id) = row.get(0) {
                        ep_ids.push(id);
                    }
                }
            }
        }

        // Delete Media (Due to ON DELETE CASCADE and PRAGMA foreign_keys = ON, this will automatically
        // delete all related rows in Episodes, History, and Local_Files)
        // Code Review Guard: explicitly forbidding std::fs::remove_file or std::fs::remove_dir in this flow
        // WAIT, the prompt says: "If an episode is deleted from the tracker, the Rust remove_show logic uses this naming convention to target and delete specific files from the hard drive instantly."
        if let Err(e) = tx.execute("DELETE FROM Media WHERE id = ?", [media_id]) {
            let _ = tx.rollback();
            let _ = app.emit(
                "media-delete-failed",
                json!({ "media_id": media_id, "error": e.to_string() }),
            );
            return;
        }

        // Delete still images for all episodes
        let still_dir = crate::tmdb::get_still_cache_dir();
        for ep_id in ep_ids {
            let filename = format!("ep_{}.jpg", ep_id);
            let filepath = still_dir.join(filename);
            if filepath.exists() {
                let _ = std::fs::remove_file(filepath);
            }
        }

        if let Err(e) = tx.commit() {
            let _ = app.emit(
                "media-delete-failed",
                json!({ "media_id": media_id, "error": e.to_string() }),
            );
            return;
        }

        let _ = app.emit("media-deleted", json!({ "media_id": media_id }));
    });

    Ok(())
}

pub fn validate_and_stage_restore(
    backup_path: &std::path::Path,
    app_dir: &std::path::Path,
) -> Result<(), AppError> {
    if !backup_path.exists() {
        return Err(AppError::Custom("Backup file does not exist".to_string()));
    }

    // Cryptographic SHA-256 Check: verify sidecar checksum if present
    let verified_hash = if let Some(sidecar_path) = crate::backup::find_sidecar_path(backup_path) {
        let sidecar_content = std::fs::read_to_string(&sidecar_path)
            .map_err(|e| AppError::Custom(format!("Failed to read SHA-256 sidecar file: {}", e)))?;
        let expected_hash = sidecar_content
            .split_whitespace()
            .next()
            .ok_or_else(|| AppError::Custom("SHA-256 sidecar file is empty".to_string()))?
            .to_lowercase();

        let actual_hash = crate::backup::compute_sha256(backup_path)?;
        if expected_hash != actual_hash {
            return Err(AppError::Custom(format!(
                "SHA-256 checksum verification failed for restore candidate: expected {}, got {}",
                expected_hash, actual_hash
            )));
        }
        tracing::info!(action = "restore_validation", hash = %actual_hash, "SHA-256 sidecar verification passed");
        Some(actual_hash)
    } else {
        tracing::warn!(action = "restore_validation", path = ?backup_path, "No SHA-256 sidecar found; proceeding with PRAGMA integrity_check only");
        None
    };

    // Pragma Check: verify it's a valid SQLite database
    match rusqlite::Connection::open_with_flags(
        backup_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(conn) => {
            let mut stmt = conn.prepare("PRAGMA integrity_check").map_err(|e| {
                AppError::Custom(format!("Failed to prepare integrity check: {}", e))
            })?;
            let mut rows = stmt.query([]).map_err(|e| {
                AppError::Custom(format!("Failed to execute integrity check: {}", e))
            })?;
            if let Some(row) = rows.next().unwrap_or(None) {
                let result: String = row.get(0).unwrap_or_default();
                if result != "ok" {
                    return Err(AppError::Custom(
                        "Backup file is not a valid SQLite database (integrity check failed)"
                            .to_string(),
                    ));
                }
            } else {
                return Err(AppError::Custom(
                    "Backup file is not a valid SQLite database".to_string(),
                ));
            }
        }
        Err(e) => {
            return Err(AppError::Custom(format!("Failed to open backup file for validation. It may be locked by another program or corrupted: {}", e)));
        }
    }

    // Prepare paths
    let db_dir = app_dir.join("db");
    if !db_dir.exists() {
        std::fs::create_dir_all(&db_dir)
            .map_err(|e| AppError::Custom(format!("Failed to create db dir: {}", e)))?;
    }
    let pending_db_path = db_dir.join("watchmark.db.pending");
    let trigger_file_path = app_dir.join(".restore_pending");

    // Copy to pending path
    std::fs::copy(backup_path, &pending_db_path).map_err(|e| {
        AppError::Custom(format!(
            "Failed to stage backup file (possibly out of space): {}",
            e
        ))
    })?;

    // Fsync staged pending database file
    if let Ok(file) = std::fs::File::open(&pending_db_path) {
        let _ = file.sync_all();
    }

    // Stage companion SHA-256 sidecar for the pending database
    let hash_to_stage = match verified_hash {
        Some(h) => h,
        None => crate::backup::compute_sha256(&pending_db_path).unwrap_or_default(),
    };
    if !hash_to_stage.is_empty() {
        let _ = crate::backup::write_sha256_sidecar(&pending_db_path, &hash_to_stage);
    }

    // Create trigger file
    std::fs::write(&trigger_file_path, b"pending_restore")
        .map_err(|e| AppError::Custom(format!("Failed to write restore trigger flag: {}", e)))?;

    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app_handle))]
pub async fn prepare_restore(
    backup_path: String,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let backup_path_buf = std::path::PathBuf::from(&backup_path);
    let app_dir = crate::db::get_app_data_dir();

    validate_and_stage_restore(&backup_path_buf, &app_dir)?;

    // Delay restart slightly to allow the frontend to receive the success response
    // and prevent throwing an IPC error that triggers the catch block
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        app_handle.restart();
    });

    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn export_database(
    target_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let target_path_buf = std::path::PathBuf::from(&target_path);

    // Ensure we can use the file path (must remove if exists since VACUUM INTO fails otherwise)
    if target_path_buf.exists() {
        if let Err(e) = std::fs::remove_file(&target_path_buf) {
            return Err(AppError::Custom(format!(
                "Failed to remove existing file at destination: {}",
                e
            )));
        }
    }

    let db_queue = state.db_queue.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let export_path = target_path_buf.clone();

    db_queue.push_high_priority(move |conn| {
        let sql = format!(
            "VACUUM INTO '{}'",
            export_path.to_string_lossy().replace("'", "''")
        );
        let result = conn
            .execute(&sql, [])
            .map_err(|e| AppError::Custom(format!("Database export failed: {}", e)));
        let _ = tx.send(result);
    });

    match rx.await {
        Ok(Ok(_)) => {
            // Write SHA-256 companion sidecar file upon successful export
            if let Ok(hash) = crate::backup::compute_sha256(&target_path_buf) {
                let _ = crate::backup::write_sha256_sidecar(&target_path_buf, &hash);
            }
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AppError::Custom("Database worker dropped task".to_string())),
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub fn get_media_details_db(
    media_id: i32,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Value, AppError> {
    tracing::info!("[BACKEND] 🧠 Extracting detailed Media and Season data...");
    let mut backdrop_size = "w1280".to_string();
    let mut _poster_size = "w500".to_string();
    if let Ok(Some(monitor)) = app.primary_monitor() {
        if monitor.scale_factor() > 1.0 {
            backdrop_size = "original".to_string();
            _poster_size = "original".to_string();
        }
    }
    let high_performance_mode = state.settings.read().unwrap().high_performance_mode;

    handle_panic(|| {
        let conn = crate::db::get_readonly_connection()?;
        let new_seasons = load_new_seasons(&conn);

        let mut stmt = conn.prepare("SELECT * FROM Media WHERE id=?")?;
        let mut media: Option<Value> = None;

        if let Ok(mut rows) = stmt.query(params![media_id]) {
            if let Ok(Some(row)) = rows.next() {
                let m_type: String = row.get(2).unwrap_or_default();
                let raw_release_date: String = row
                    .get::<_, Option<String>>(11)
                    .unwrap_or_default()
                    .unwrap_or_default();
                let (sanitized_release_date, is_exact, is_known) =
                    if raw_release_date.is_empty() && m_type != "TV" {
                        ("0000-00-00".to_string(), false, false)
                    } else {
                        crate::sanitizer::sanitize_date(&raw_release_date)
                    };

                let is_unaired = if is_known
                    && !sanitized_release_date.is_empty()
                    && sanitized_release_date != "0000-00-00"
                {
                    let now = chrono::Utc::now().naive_utc().date();
                    if let Ok(parsed) =
                        chrono::NaiveDate::parse_from_str(&sanitized_release_date, "%Y-%m-%d")
                    {
                        parsed > now
                    } else {
                        false
                    }
                } else {
                    false
                };

                let raw_synopsis = row
                    .get::<_, Option<String>>(4)
                    .unwrap_or_default()
                    .unwrap_or_default();
                let sanitized_synopsis =
                    crate::sanitizer::sanitize_text(&raw_synopsis, "No overview available.");

                let collection_id = row.get::<_, Option<i32>>(15).unwrap_or_default();
                let mut collection_parts = serde_json::Value::Null;

                if let Some(c_id) = collection_id {
                    if let Ok(mut c_stmt) = conn.prepare("SELECT parts FROM Collections WHERE id=?")
                    {
                        if let Ok(mut c_rows) = c_stmt.query(params![c_id]) {
                            if let Ok(Some(c_row)) = c_rows.next() {
                                let parts_str: String =
                                    c_row.get(0).unwrap_or_else(|_| "[]".to_string());
                                collection_parts = serde_json::from_str(&parts_str)
                                    .unwrap_or(serde_json::Value::Null);
                            }
                        }
                    }
                }

                media = Some(json!({
                    "id": row.get::<_, i32>(0).unwrap_or(0),
                    "tmdb_id": row.get::<_, String>(1).unwrap_or_default(),
                    "type": m_type,
                    "title": row.get::<_, Option<String>>(3).unwrap_or_default().unwrap_or_default(),
                    "synopsis": sanitized_synopsis,
                    "poster_path": (|| {
                        let raw = row.get::<_, Option<String>>(5).unwrap_or_default().unwrap_or_default();
                        crate::tmdb::resolve_local_poster_path(&raw, "w500", high_performance_mode).unwrap_or(raw)
                    })(),
                    "backdrop_path": (|| {
                        let raw = row.get::<_, Option<String>>(6).unwrap_or_default().unwrap_or_default();
                        crate::tmdb::resolve_local_backdrop_path(&raw, &backdrop_size, high_performance_mode).unwrap_or(raw)
                    })(),
                    "total_episodes": row.get::<_, Option<i32>>(7).unwrap_or_default().unwrap_or(0),
                    "status": row.get::<_, Option<String>>(8).unwrap_or_default().unwrap_or_default(),
                    "vote_average": row.get::<_, Option<f64>>(9).unwrap_or_default().unwrap_or(0.0),
                    "user_rating": row.get::<_, Option<i32>>(10).unwrap_or_default(),
                    "release_date": sanitized_release_date,
                    "is_exact_date": is_exact,
                    "is_date_known": is_known,
                    "is_unaired": is_unaired,
                    "collection_id": collection_id,
                    "collection_name": row.get::<_, Option<String>>(16).unwrap_or_default(),
                    "collection_parts": collection_parts,
                }));
            }
        }

        if let Some(mut m) = media {
            let m_type = m["type"].as_str().unwrap_or_default().to_string();

            // Watched count
            let watched_eps: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM Episodes WHERE media_id=? AND status='Completed' AND season_num > 0",
                    params![media_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            m["completed_eps"] = json!(watched_eps);

            let total_available: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM Episodes WHERE media_id=? AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now'))",
                    params![media_id],
                    |r| r.get(0),
                )
                .unwrap_or(m["total_episodes"].as_i64().unwrap_or(0) as i32);

            m["total_available"] = json!(total_available);

            let mut seasons = vec![];
            if m_type == "TV" {
                let mut s_stmt = conn.prepare(
                    "SELECT DISTINCT season_num FROM Episodes WHERE media_id=? ORDER BY season_num",
                )?;
                let s_rows = s_stmt.query_map(params![media_id], |row| row.get::<_, u32>(0));
                if let Ok(s_rows_iter) = s_rows {
                    for s in s_rows_iter.flatten() {
                        seasons.push(s);
                    }
                }
            } else {
                seasons.push(1);
            }
            m["seasons"] = json!(seasons);
            m["new_season"] = json!(new_seasons.get(&media_id).copied());

            let mut eps_stmt = conn.prepare(
                "
                SELECT e.id, e.media_id, e.season_num, e.ep_num, e.title, e.runtime,
                       e.still_path, e.overview, e.season_overview, e.watch_count,
                       e.last_position, e.status, e.completed_date, e.air_date,
                       e.is_exact_date, e.is_air_date_manual, l.file_path
                FROM Episodes e
                LEFT JOIN Local_Files l ON e.id = l.episode_id
                WHERE e.media_id=?
                ORDER BY e.season_num ASC, e.ep_num ASC
            ",
            )?;

            let mut episodes = vec![];
            if let Ok(ep_rows) = eps_stmt.query_map(params![media_id], |row| {
                let ep_id = row.get::<_, i32>(0)?;
                let raw_air_date: String = row.get::<_, Option<String>>(13)?.unwrap_or_default();
                let (sanitized_air_date, is_exact, is_known) =
                    crate::sanitizer::sanitize_date(&raw_air_date);

                let is_unaired = if is_known && !sanitized_air_date.is_empty() {
                    let now = chrono::Utc::now().naive_utc().date();
                    if let Ok(parsed) =
                        chrono::NaiveDate::parse_from_str(&sanitized_air_date, "%Y-%m-%d")
                    {
                        parsed > now
                    } else {
                        false
                    }
                } else {
                    false
                };

                let potential_spoiler = if is_known && !sanitized_air_date.is_empty() {
                    let now = chrono::Utc::now().naive_utc().date();
                    if let Ok(parsed) =
                        chrono::NaiveDate::parse_from_str(&sanitized_air_date, "%Y-%m-%d")
                    {
                        let duration = now.signed_duration_since(parsed);
                        duration.num_days() <= 2
                    } else {
                        false
                    }
                } else {
                    false
                };

                let raw_overview = row.get::<_, Option<String>>(7)?.unwrap_or_default();
                let sanitized_overview =
                    crate::sanitizer::sanitize_text(&raw_overview, "No episode summary.");

                let raw_season_overview = row.get::<_, Option<String>>(8)?.unwrap_or_default();

                let runtime: i32 = row.get::<_, Option<i32>>(5)?.unwrap_or(0);
                let last_position: i32 = row.get::<_, Option<i32>>(10)?.unwrap_or(0);
                let progress_percentage =
                    crate::sanitizer::calculate_progress_percentage(last_position, runtime);

                let raw_still_path: String = row.get::<_, Option<String>>(6)?.unwrap_or_default();
                let is_fallback_image = raw_still_path.is_empty();

                let resolved_still_path = if !is_fallback_image {
                    crate::tmdb::resolve_local_still_path(&raw_still_path, ep_id)
                        .unwrap_or(raw_still_path)
                } else {
                    m["backdrop_path"].as_str().unwrap_or_default().to_string()
                };

                Ok(json!({
                    "id": ep_id,
                    "media_id": row.get::<_, i32>(1)?,
                    "season_num": row.get::<_, u32>(2)?,
                    "ep_num": row.get::<_, u32>(3)?,
                    "title": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    "runtime": runtime,
                    "still_path": resolved_still_path,
                    "overview": sanitized_overview,
                    "season_overview": raw_season_overview,
                    "watch_count": row.get::<_, Option<i32>>(9)?.unwrap_or(0),
                    "last_position": last_position,
                    "status": row.get::<_, Option<String>>(11)?.unwrap_or_default(),
                    "completed_date": row.get::<_, Option<String>>(12)?.unwrap_or_default(),
                    "air_date": sanitized_air_date,
                    "is_exact_date": is_exact,
                    "is_date_known": is_known,
                    "file_path": row.get::<_, Option<String>>(16)?.unwrap_or_default(),
                    "is_unaired": is_unaired,
                    "progress_percentage": progress_percentage,
                    "is_fallback_image": is_fallback_image,
                    "potential_spoiler": potential_spoiler,
                }))
            }) {
                for ep in ep_rows.flatten() {
                    episodes.push(ep);
                }
            }

            m["episodes"] = json!(episodes);
            Ok(m)
        } else {
            Err(AppError::Custom("Media not found".to_string()))
        }
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn add_to_tracker(
    tmdb_id: String,
    media_type: String,
    archive: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    if !state.is_api_authorized.load(Ordering::SeqCst) {
        return Err(AppError::Custom(
            "API Key is invalid or unauthorized.".to_string(),
        ));
    }

    let settings = crate::settings::load_settings().map_err(|e| AppError::Custom(e))?;
    if settings.tmdb_api_key.is_empty() {
        return Err(AppError::Custom(
            "Missing TMDB API Key. Please add it in Settings.".to_string(),
        ));
    }

    let valid_media_type = crate::models::MediaType::from_str(&media_type)
        .as_str()
        .to_string();
    let details =
        match crate::tmdb::get_media_details(&settings.tmdb_api_key, &tmdb_id, &valid_media_type)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                if let AppError::NetworkBlocked = e {
                    state.is_api_authorized.store(false, Ordering::SeqCst);
                    state.db_queue.clear();
                    let _ = app.emit("api-auth-failed", ());
                    return Err(AppError::Custom("API Key was revoked.".to_string()));
                }
                return Err(e);
            }
        };

    let api_key = settings.tmdb_api_key.clone();
    let media_type_clone = valid_media_type.clone();
    let tmdb_id_clone = tmdb_id.clone();

    let app_clone_for_task = app.clone();

    // Detect High-DPI displays dynamically for backdrop high-resolution support
    let mut backdrop_size = "w1280".to_string();
    let mut poster_size = "w500".to_string();
    if let Ok(Some(monitor)) = app.primary_monitor() {
        if monitor.scale_factor() > 1.0 {
            backdrop_size = "original".to_string();
            poster_size = "original".to_string();
        }
    }
    let image_config = crate::tmdb::ImageConfig {
        backdrop_size,
        poster_size,
    };

    // Spawn a dedicated background task that does NOT block the main thread and can handle async fetch loops
    let task = tokio::task::spawn(async move {
        use tauri::Manager;
        let state_clone = app_clone_for_task.state::<AppState>();
        let high_performance_mode = state_clone.settings.read().unwrap().high_performance_mode;
        let failed_syncs_clone = state_clone.failed_image_syncs.clone();
        let cancel_tokens_clone = state_clone.cancel_tokens.clone();
        // Run DB operation inside spawn_blocking to insert the Media row first
        let media_id_res = tokio::task::spawn_blocking({
            let details = details.clone();
            let valid_media_type = valid_media_type.clone();
            let tmdb_id_clone = tmdb_id_clone.clone();
            let failed_syncs_clone = failed_syncs_clone.clone();
            move || {
                handle_panic(std::panic::AssertUnwindSafe(|| {
                    let mut synopsis = details["synopsis"].as_str().unwrap_or("").to_string();
                    if synopsis.chars().count() > 10000 {
                        synopsis = synopsis.chars().take(10000).collect::<String>();
                        synopsis.push_str("...");
                    }

                    if let Some(poster) = details["poster_path"].as_str() {
                        if !poster.trim().is_empty() {
                            let poster_str = poster.to_string();
                            let failed_queue = failed_syncs_clone.clone();
                            let size = image_config.poster_size.clone();
                            tokio::spawn(async move {
                                if crate::tmdb::download_image(&poster_str, &size, high_performance_mode).await.is_none() {
                                    if let Ok(mut queue) = failed_queue.write() {
                                        queue.insert((poster_str, size.clone()));
                                    }
                                }
                            });
                        }
                    }
                    if let Some(backdrop) = details["backdrop_path"].as_str() {
                        if !backdrop.trim().is_empty() {
                            let backdrop_str = backdrop.to_string();
                            let failed_queue = failed_syncs_clone.clone();
                            let size = image_config.backdrop_size.clone();
                            tokio::spawn(async move {
                                if crate::tmdb::download_image(&backdrop_str, &size, high_performance_mode).await.is_none() {
                                    if let Ok(mut queue) = failed_queue.write() {
                                        queue.insert((backdrop_str, size.clone()));
                                    }
                                }
                            });
                        }
                    }

                    if let Ok(mut conn) = get_db_connection() {
                        if let Ok(tx) = conn.transaction() {
                            let mut existing_id = None;
                            {
                                if let Ok(mut stmt) = tx.prepare("SELECT id FROM Media WHERE tmdb_id=?") {
                                    if let Ok(mut rows) = stmt.query(params![tmdb_id_clone]) {
                                        if let Ok(Some(row)) = rows.next() {
                                            existing_id = Some(row.get::<_, i32>(0).unwrap_or(0));
                                        }
                                    }
                                }
                            }

                            let media_id = if let Some(id) = existing_id {
                                let _ = tx.execute(
                                    "UPDATE Media SET \"title\" = ?, synopsis = ?, poster_path = ?, backdrop_path = ?, total_episodes = ?, vote_average = ?, release_date = ?, is_exact_date = ?, genres = ?, networks = ?, collection_id = ?, collection_name = ?, backdrop_fallback = ?
                                     WHERE id = ?",
                                    params![
                                        details["title"].as_str().unwrap_or("Unknown Title"),
                                        synopsis,
                                        if details["poster_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["poster_path"].as_str().unwrap_or("")) },
                                        if details["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["backdrop_path"].as_str().unwrap_or("")) },
                                        details["total_episodes"].as_i64().unwrap_or(1) as i32,
                                        details["vote_average"].as_f64().unwrap_or(0.0),
                                        if details["release_date"].as_str().unwrap_or("") == "0000-00-00" { None } else { Some(details["release_date"].as_str().unwrap_or("")) },
                                        details["is_exact_date"].as_bool().unwrap_or(true),
                                        details["genres"].as_str().unwrap_or(""),
                                        details["networks"].as_str().unwrap_or(""),
                                        details["collection_id"].as_i64().map(|id| id as i32),
                                        details["collection_name"].as_str(),
                                        if details["fallback_type"].as_str().unwrap_or("").is_empty() { None } else { Some(details["fallback_type"].as_str().unwrap_or("")) },
                                        id
                                    ]
                                );
                                id
                            } else {
                                let _ = tx.execute(
                                    "INSERT INTO Media (tmdb_id, \"type\", \"title\", synopsis, poster_path, backdrop_path, total_episodes, status, vote_average, release_date, is_exact_date, genres, networks, collection_id, collection_name, backdrop_fallback)
                                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(tmdb_id, \"type\") DO UPDATE SET
                                        \"title\"=excluded.\"title\", synopsis=excluded.synopsis, poster_path=excluded.poster_path,
                                        backdrop_path=excluded.backdrop_path, total_episodes=excluded.total_episodes,
                                        vote_average=excluded.vote_average, release_date=excluded.release_date, is_exact_date=excluded.is_exact_date,
                                        genres=excluded.genres, networks=excluded.networks, collection_id=excluded.collection_id, collection_name=excluded.collection_name, backdrop_fallback=excluded.backdrop_fallback",
                                    params![
                                        tmdb_id_clone,
                                        valid_media_type,
                                        details["title"].as_str().unwrap_or("Unknown Title"),
                                        synopsis,
                                        if details["poster_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["poster_path"].as_str().unwrap_or("")) },
                                        if details["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["backdrop_path"].as_str().unwrap_or("")) },
                                        details["total_episodes"].as_i64().unwrap_or(1) as i32,
                                        details["status"].as_str().unwrap_or("Plan to Watch"),
                                        details["vote_average"].as_f64().unwrap_or(0.0),
                                        if details["release_date"].as_str().unwrap_or("") == "0000-00-00" { None } else { Some(details["release_date"].as_str().unwrap_or("")) },
                                        details["is_exact_date"].as_bool().unwrap_or(true),
                                        details["genres"].as_str().unwrap_or(""),
                                        details["networks"].as_str().unwrap_or(""),
                                        details["collection_id"].as_i64().map(|id| id as i32),
                                        details["collection_name"].as_str(),
                                        if details["fallback_type"].as_str().unwrap_or("").is_empty() { None } else { Some(details["fallback_type"].as_str().unwrap_or("")) }
                                    ]
                                );

                                let mut new_id = tx.last_insert_rowid() as i32;
                                if new_id == 0 {
                                    if let Ok(mut stmt) = tx.prepare("SELECT id FROM Media WHERE tmdb_id=? AND \"type\"=?") {
                                        if let Ok(mut rows) = stmt.query(params![tmdb_id_clone, valid_media_type]) {
                                            if let Ok(Some(row)) = rows.next() {
                                                new_id = row.get::<_, i32>(0).unwrap_or(0);
                                            }
                                        }
                                    }
                                }
                                new_id
                            };
                            let _ = tx.commit();
                            Ok::<i32, AppError>(media_id)
                        } else {
                            Err(AppError::Custom("DB transaction error".to_string()))
                        }
                    } else {
                        Err(AppError::Custom("DB connection error".to_string()))
                    }
                }))
            }
        }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())))?;

        let media_id = media_id_res;

        let token = CancellationToken::new();
        {
            let mut tokens = cancel_tokens_clone.write().unwrap();
            tokens.insert(media_id.to_string(), token.clone());
        }

        let ep_status = if archive { "Completed" } else { "Unwatched" };
        let ep_watch_count = if archive { 1 } else { 0 };

        if media_type_clone == "TV" {
            // Iterative season fetching and inserting
            if let Some(seasons) = details["seasons"].as_array() {
                let total_seasons = seasons.len();
                for (index, season) in seasons.iter().enumerate() {
                    if let Some(s_num) = season["season_number"].as_i64() {
                        if s_num >= 0 {
                            // Rate limit check
                            if token.is_cancelled() {
                                tracing::info!(
                                    "[IPC: CANCEL] 🛑 Fetch loop cancelled for media_id {}",
                                    media_id
                                );
                                break;
                            }

                            // Rate limit check
                            // check_rate_limit uses state, let's just use app_clone_for_task.state::<AppState>() here since we are back in async context
                            crate::commands::check_rate_limit(
                                &app_clone_for_task.state::<AppState>(),
                            )
                            .await;

                            // Yield back to executor to prevent blocking the async runtime
                            tokio::task::yield_now().await;

                            // 100ms deterministic delay
                            if index > 0 {
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }

                            let _ = app_clone_for_task.emit(
                                "sync-progress",
                                json!({
                                    "mediaId": media_id,
                                    "tmdbId": tmdb_id_clone,
                                    "currentSeason": index + 1,
                                    "totalSeasons": total_seasons
                                }),
                            );

                            match crate::tmdb::get_tv_season_episodes(
                                &api_key,
                                &tmdb_id_clone,
                                s_num as u32,
                            )
                            .await
                            {
                                Ok(eps) => {
                                    // Insert the chunk immediately inside spawn_blocking
                                    let details_clone = details.clone();
                                    let res = tokio::task::spawn_blocking(move || {
                                    handle_panic(|| {
                                        let mut inserted_eps = Vec::new();
                                        if let Ok(mut conn) = get_db_connection() {
                                            if let Ok(tx) = conn.transaction() {
                                                for ep in eps {
                                                    let mut ep_overview = ep["overview"].as_str().unwrap_or("").to_string();
                                                    if ep_overview.chars().count() > 10000 {
                                                        ep_overview = ep_overview.chars().take(10000).collect::<String>();
                                                        ep_overview.push_str("...");
                                                    }

                                                    let mut season_overview = ep["season_overview"].as_str().unwrap_or("").to_string();
                                                    if season_overview.chars().count() > 10000 {
                                                        season_overview = season_overview.chars().take(10000).collect::<String>();
                                                        season_overview.push_str("...");
                                                    }

                                                    let season_num = ep["season_num"].as_i64().unwrap_or(1) as u32;
                                                    let ep_num = ep["ep_num"].as_i64().unwrap_or(1) as u32;

                                                    let mut should_update_air_date = true;
                                                    if let Ok(mut stmt) = tx.prepare("SELECT is_air_date_manual FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                        if let Ok(mut rows) = stmt.query(params![media_id, season_num, ep_num]) {
                                                            if let Ok(Some(row)) = rows.next() {
                                                                let is_manual: bool = row.get(0).unwrap_or(false);
                                                                if is_manual {
                                                                    should_update_air_date = false;
                                                                }
                                                            }
                                                        }
                                                    }

                                                    let new_air_date = ep["air_date"].as_str().unwrap_or("");
                                                    let valid_air_date = if new_air_date.to_uppercase() == "TBD" || (new_air_date.len() > 0 && NaiveDate::parse_from_str(new_air_date, "%Y-%m-%d").is_err() && new_air_date.len() != 4) {
                                                        should_update_air_date = false;
                                                        ""
                                                    } else {
                                                        new_air_date
                                                    };

                                                    let raw_still_path = if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) };

                                                    if should_update_air_date {
                                                        let _ = tx.execute(
                                                            "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count, air_date, is_exact_date)
                                                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                             ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                overview=excluded.overview, season_overview=excluded.season_overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                                            params![
                                                                media_id,
                                                                season_num,
                                                                ep_num,
                                                                ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                ep.get("runtime").and_then(|r| r.as_i64()).unwrap_or(details_clone.get("global_runtime").and_then(|gr| gr.as_i64()).unwrap_or(0)) as i32,
                                                                raw_still_path,
                                                                ep_overview,
                                                                season_overview,
                                                                ep_status,
                                                                ep_watch_count,
                                                                valid_air_date,
                                                                ep["is_exact_date"].as_bool().unwrap_or(true)
                                                            ]
                                                        );
                                                    } else {
                                                        let _ = tx.execute(
                                                            "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count)
                                                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                             ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                overview=excluded.overview, season_overview=excluded.season_overview",
                                                            params![
                                                                media_id,
                                                                season_num,
                                                                ep_num,
                                                                ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                ep.get("runtime").and_then(|r| r.as_i64()).unwrap_or(details_clone.get("global_runtime").and_then(|gr| gr.as_i64()).unwrap_or(0)) as i32,
                                                                raw_still_path,
                                                                ep_overview,
                                                                season_overview,
                                                                ep_status,
                                                                ep_watch_count
                                                            ]
                                                        );
                                                    }

                                                    if let Some(path) = raw_still_path {
                                                        if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                            if let Ok(mut rows) = stmt.query(params![media_id, season_num, ep_num]) {
                                                                if let Ok(Some(row)) = rows.next() {
                                                                    let ep_id: i32 = row.get(0).unwrap_or(0);
                                                                    inserted_eps.push((ep_id, path.to_string()));
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                let _ = tx.commit();
                                            }
                                        }
                                        Ok::<Vec<(i32, String)>, AppError>(inserted_eps)
                                    })
                                }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

                                    if let Ok(inserted_eps) = res {
                                        if !inserted_eps.is_empty() {
                                            // "Lazy Discovery" architecture: Priority active-season vs background idle downloading
                                            let is_active_season = index == 0;
                                            let token_clone = token.clone();

                                            let download_future = async move {
                                                let mut tasks = Vec::new();

                                                for (ep_id, path) in inserted_eps {
                                                    if token_clone.is_cancelled() {
                                                        break;
                                                    }

                                                    let permit =
                                                        crate::tmdb::GLOBAL_IMAGE_SEMAPHORE
                                                            .acquire()
                                                            .await
                                                            .unwrap();
                                                    let path_clone = path.clone();
                                                    let ep_id_clone = ep_id;

                                                    tasks.push(tokio::spawn(async move {
                                                        crate::tmdb::download_episode_still(
                                                            &path_clone,
                                                            ep_id_clone,
                                                            high_performance_mode,
                                                        )
                                                        .await;
                                                        drop(permit);
                                                    }));
                                                }

                                                for t in tasks {
                                                    let _ = t.await;
                                                }
                                            };

                                            if is_active_season {
                                                // Await inline for the active season
                                                download_future.await;
                                            } else {
                                                // Spawn background task for idle seasons
                                                tokio::spawn(download_future);
                                            }
                                        }
                                    }
                                }
                                Err(AppError::Custom(err)) if err == "NOT_FOUND" => {
                                    tracing::warn!("[API] ⚠️ Season {} missing (404) for show ID {}. Skipping.", s_num, tmdb_id_clone);
                                    let _ = app_clone_for_task.emit(
                                        "warning-toast",
                                        format!("Season {} missing from TMDB, skipped.", s_num),
                                    );
                                    continue;
                                }
                                Err(AppError::Custom(err)) if err.starts_with("RATE_LIMIT:") => {
                                    let parts: Vec<&str> = err.split(':').collect();
                                    let retry_after =
                                        parts.get(1).unwrap_or(&"1").parse::<u64>().unwrap_or(1);
                                    let async_state = app_clone_for_task.state::<AppState>();
                                    async_state.is_rate_limited.store(true, Ordering::SeqCst);
                                    async_state.rate_limit_reset.store(
                                        chrono::Utc::now().timestamp() + retry_after as i64,
                                        Ordering::SeqCst,
                                    );

                                    tracing::warn!(
                                        "[API] ⚠️ Rate limited. Pausing queue for {} seconds.",
                                        retry_after
                                    );
                                    tokio::time::sleep(tokio::time::Duration::from_secs(
                                        retry_after,
                                    ))
                                    .await;

                                    // re-acquire state because previous async_state reference might have been moved/borrowed elsewhere (though state is managed, so it's fine, but let's be safe)
                                    app_clone_for_task
                                        .state::<AppState>()
                                        .is_rate_limited
                                        .store(false, Ordering::SeqCst);

                                    // Retry once directly inline after sleeping
                                    if let Ok(eps) = crate::tmdb::get_tv_season_episodes(
                                        &api_key,
                                        &tmdb_id_clone,
                                        s_num as u32,
                                    )
                                    .await
                                    {
                                        let res = tokio::task::spawn_blocking({
                                            let media_id = media_id;
                                            let ep_status = ep_status;
                                            let ep_watch_count = ep_watch_count;
                                            let details = details.clone();
                                            move || {
                                                handle_panic(|| {
                                                    let mut inserted_eps = Vec::new();
                                                    if let Ok(mut conn) = get_db_connection() {
                                                        if let Ok(tx) = conn.transaction() {
                                                            for ep in eps {
                                                                let ep_num = ep["ep_num"].as_i64().unwrap_or(1) as u32;
                                                                let raw_still_path = if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) };
                                                                let _ = tx.execute(
                                                                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count, air_date, is_exact_date)
                                                                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                        \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                        overview=excluded.overview, season_overview=excluded.season_overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                                                    params![
                                                                        media_id,
                                                                        s_num,
                                                                        ep_num,
                                                                        ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                        ep.get("runtime").and_then(|r| r.as_i64()).unwrap_or(details.get("global_runtime").and_then(|gr| gr.as_i64()).unwrap_or(0)) as i32,
                                                                        raw_still_path,
                                                                        ep["overview"].as_str().unwrap_or(""),
                                                                        ep["season_overview"].as_str().unwrap_or(""),
                                                                        ep_status,
                                                                        ep_watch_count,
                                                                        ep["air_date"].as_str().unwrap_or(""),
                                                                        ep["is_exact_date"].as_bool().unwrap_or(true)
                                                                    ]
                                                                );

                                                                if let Some(path) = raw_still_path {
                                                                    if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                                        if let Ok(mut rows) = stmt.query(params![media_id, s_num, ep_num]) {
                                                                            if let Ok(Some(row)) = rows.next() {
                                                                                let ep_id: i32 = row.get(0).unwrap_or(0);
                                                                                inserted_eps.push((ep_id, path.to_string()));
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            let _ = tx.commit();
                                                        }
                                                    }
                                                    Ok::<Vec<(i32, String)>, AppError>(inserted_eps)
                                                })
                                            }
                                        }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

                                        if let Ok(inserted_eps) = res {
                                            let mut tasks = Vec::new();

                                            for (ep_id, path) in inserted_eps {
                                                if token.is_cancelled() {
                                                    break;
                                                }

                                                let permit = crate::tmdb::GLOBAL_IMAGE_SEMAPHORE
                                                    .acquire()
                                                    .await
                                                    .unwrap();
                                                let path_clone = path.clone();
                                                let ep_id_clone = ep_id;

                                                tasks.push(tokio::spawn(async move {
                                                    crate::tmdb::download_episode_still(
                                                        &path_clone,
                                                        ep_id_clone,
                                                        high_performance_mode,
                                                    )
                                                    .await;
                                                    drop(permit);
                                                }));
                                            }

                                            for t in tasks {
                                                let _ = t.await;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "[API] 🚨 Error fetching season {}: {}",
                                        s_num,
                                        e
                                    );
                                    continue;
                                }
                            }
                        }
                    }
                }
                let _ = app_clone_for_task.emit(
                    "sync-progress",
                    json!({
                        "mediaId": media_id,
                            "tmdbId": tmdb_id_clone,
                            "currentSeason": total_seasons + 1,
                        "totalSeasons": total_seasons
                    }),
                );
            }
        } else {
            if !token.is_cancelled() {
                // It's a Movie, just insert the single "Episode" via spawn_blocking
                let details_clone = details.clone();
                let _ = tokio::task::spawn_blocking(move || {
                handle_panic(|| {
                    if let Ok(mut conn) = get_db_connection() {
                        if let Ok(tx) = conn.transaction() {
                            let mut should_update_air_date = true;
                            if let Ok(mut stmt) = tx.prepare("SELECT is_air_date_manual FROM Episodes WHERE media_id=? AND season_num=1 AND ep_num=1") {
                                if let Ok(mut rows) = stmt.query(params![media_id]) {
                                    if let Ok(Some(row)) = rows.next() {
                                        let is_manual: bool = row.get(0).unwrap_or(false);
                                        if is_manual {
                                            should_update_air_date = false;
                                        }
                                    }
                                }
                            }

                            let new_air_date = details_clone["release_date"].as_str().unwrap_or("");
                            let valid_air_date = if new_air_date.to_uppercase() == "TBD" || (new_air_date.len() > 0 && NaiveDate::parse_from_str(new_air_date, "%Y-%m-%d").is_err() && new_air_date.len() != 4) {
                                should_update_air_date = false;
                                ""
                            } else {
                                new_air_date
                            };

                            let mut synopsis = details_clone["synopsis"].as_str().unwrap_or("").to_string();
                            if synopsis.chars().count() > 10000 {
                                synopsis = synopsis.chars().take(10000).collect::<String>();
                                synopsis.push_str("...");
                            }

                            if should_update_air_date {
                                let _ = tx.execute(
                                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, status, watch_count, air_date, is_exact_date)
                                     VALUES (?, 1, 1, ?, ?, ?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                        \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                        overview=excluded.overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                    params![
                                        media_id,
                                        details_clone["title"].as_str().unwrap_or("Unknown Title"),
                                        details_clone["runtime"].as_i64().unwrap_or(0) as i32,
                                        if details_clone["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details_clone["backdrop_path"].as_str().unwrap_or("")) },
                                        synopsis,
                                        ep_status,
                                        ep_watch_count,
                                        valid_air_date,
                                        details_clone["is_exact_date"].as_bool().unwrap_or(true)
                                    ]
                                );
                            } else {
                                let _ = tx.execute(
                                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, status, watch_count)
                                     VALUES (?, 1, 1, ?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                        \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                        overview=excluded.overview",
                                    params![
                                        media_id,
                                        details_clone["title"].as_str().unwrap_or("Unknown Title"),
                                        details_clone["runtime"].as_i64().unwrap_or(0) as i32,
                                        if details_clone["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details_clone["backdrop_path"].as_str().unwrap_or("")) },
                                        synopsis,
                                        ep_status,
                                        ep_watch_count
                                    ]
                                );
                            }
                            let _ = tx.commit();
                        }
                    }
                    Ok::<(), AppError>(())
                })
            }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

                if let Some(c_id) = details.get("collection_id").and_then(|v| v.as_i64()) {
                    tokio::task::yield_now().await;
                    if let Ok(col) =
                        crate::tmdb::get_collection_details(&api_key, c_id as i32).await
                    {
                        let _ = tokio::task::spawn_blocking(move || {
                        handle_panic(|| {
                            if let Ok(conn) = get_db_connection() {
                                let parts_str = col.get("parts").map(|p| p.to_string()).unwrap_or_else(|| "[]".to_string());
                                let _ = conn.execute(
                                    "INSERT INTO Collections (id, name, overview, poster_path, backdrop_path, parts)
                                     VALUES (?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(id) DO UPDATE SET
                                     name=excluded.name, overview=excluded.overview, poster_path=excluded.poster_path, backdrop_path=excluded.backdrop_path, parts=excluded.parts",
                                    params![
                                        c_id as i32,
                                        col.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown Collection"),
                                        crate::sanitizer::sanitize_text(col.get("overview").and_then(|v| v.as_str()).unwrap_or(""), "No overview available."),
                                        if col.get("poster_path").and_then(|v| v.as_str()).unwrap_or("").is_empty() { None } else { Some(col.get("poster_path").and_then(|v| v.as_str()).unwrap_or("")) },
                                        if col.get("backdrop_path").and_then(|v| v.as_str()).unwrap_or("").is_empty() { None } else { Some(col.get("backdrop_path").and_then(|v| v.as_str()).unwrap_or("")) },
                                        parts_str
                                    ]
                                );
                            }
                            Ok::<(), AppError>(())
                        })
                    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));
                    }
                }
            }
        }

        {
            let mut tokens = cancel_tokens_clone.write().unwrap();
            tokens.remove(&media_id.to_string());
        }

        Ok::<(), AppError>(())
    });

    match tokio::time::timeout(std::time::Duration::from_secs(15), task).await {
        Ok(res) => res.unwrap_or(Err(AppError::Custom("Task panicked".to_string()))),
        Err(_) => Err(AppError::Custom("Task Timed Out".to_string())),
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn mark_season_watched(
    media_id: i32,
    season_num: u32,
    archive_mode: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    if let Ok(mut cache) = state.stats_cache.write() {
        *cache = None;
    }

    tokio::task::spawn_blocking(move || {
        handle_panic(|| {
            let mut conn = crate::db::get_db_connection()?;
            let tx = conn.transaction()?;

            let mut episode_ids = Vec::new();
            {
                // Fetch episodes to update
                let mut stmt = tx.prepare("SELECT id FROM Episodes WHERE media_id = ? AND season_num = ? AND status != 'Completed'")?;
                let mut rows = stmt.query(params![media_id, season_num])?;

                while let Ok(Some(row)) = rows.next() {
                    episode_ids.push(row.get::<_, i32>(0).unwrap_or(0));
                }
            }

            // Update status and reset last_position
            let _ = tx.execute(
                "UPDATE Episodes SET status = 'Completed', watch_count = watch_count + 1, last_position = 0 WHERE media_id = ? AND season_num = ? AND status != 'Completed'",
                params![media_id, season_num]
            );

            // Generate history entries if NOT archiving
            if !archive_mode && !episode_ids.is_empty() {
                let session_id = uuid::Uuid::new_v4().to_string();
                let current_timestamp = chrono::Utc::now().timestamp();

                for ep_id in episode_ids {
                    let _ = tx.execute(
                        "INSERT INTO History (episode_id, timestamp, is_legacy, session_id, status) VALUES (?, ?, 0, ?, 'Completed')",
                        params![ep_id, current_timestamp, session_id]
                    );
                }
            }

            tx.commit()?;
            Ok(())
        })
    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn archive_season(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    media_id: i32,
    season_num: u32,
    is_legacy: bool,
    timestamp: Option<i64>,
) -> Result<(), AppError> {
    let db_queue = state.db_queue.clone();

    if let Ok(mut cache) = state.stats_cache.write() {
        *cache = None;
    }

    tokio::task::spawn_blocking(move || {
        let app_clone1 = app.clone();
        handle_panic(std::panic::AssertUnwindSafe(move || {
            let app = app_clone1;
            let conn = get_db_connection()?;
            let mut episode_ids = Vec::new();

            {
                let mut stmt = conn.prepare("SELECT id FROM Episodes WHERE media_id = ? AND season_num = ? ORDER BY ep_num ASC")?;
                let mut rows = stmt.query(params![media_id, season_num])?;
                while let Ok(Some(row)) = rows.next() {
                    episode_ids.push(row.get::<_, i32>(0).unwrap_or(0));
                }
            }

            if episode_ids.is_empty() {
                return Ok(());
            }

            let total_episodes = episode_ids.len();
            let base_timestamp = timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp());
            let start_timestamp = if is_legacy {
                use chrono::{Utc, TimeZone, Datelike};
                if let Some(dt) = Utc.timestamp_opt(base_timestamp, 0).single() {
                    if let Some(first_of_month) = dt.with_day(1) {
                        first_of_month.timestamp()
                    } else {
                        base_timestamp
                    }
                } else {
                    base_timestamp
                }
            } else {
                base_timestamp
            };

            let _ = app.emit("history-import-progress", json!({ "progress": 0, "total": total_episodes, "isImporting": true }));

            for (i, ep_id) in episode_ids.into_iter().enumerate() {
                let current_ep_timestamp = start_timestamp + (i as i64 * 86400);
                let app_clone = app.clone();
                let current_idx = i + 1;

                db_queue.push_low_priority(move |conn| {
                    let _ = conn.execute(
                        "UPDATE Episodes SET status = 'Completed', watch_count = watch_count + 1, last_position = 0 WHERE id = ?",
                        params![ep_id]
                    );

                    let session_id = uuid::Uuid::new_v4().to_string();
                    let legacy_int = if is_legacy { 1 } else { 0 };

                    let _ = conn.execute(
                        "INSERT INTO History (episode_id, timestamp, is_legacy, session_id, status) VALUES (?, ?, ?, ?, 'Completed')",
                        params![ep_id, current_ep_timestamp, legacy_int, session_id]
                    );

                    let progress_percent = (current_idx as f64 / total_episodes as f64 * 100.0) as i32;
                    let is_importing = current_idx < total_episodes;
                    let _ = app_clone.emit("history-import-progress", json!({ "progress": progress_percent, "total": total_episodes, "isImporting": is_importing }));
                });
            }

            Ok::<(), AppError>(())
        }))
    }).await.unwrap_or_else(|_| Err(AppError::Custom("Task panicked".to_string())))?;

    Ok(())
}

fn resolve_candidate_episode(
    conn: &rusqlite::Connection,
    media_id: i32,
    backdrop_size: &str,
    poster_size: &str,
    high_performance_mode: bool,
) -> Option<Value> {
    let mut ep_stmt = conn.prepare(
        "
        SELECT e.id, e.media_id, e.season_num, e.ep_num, e.title, e.runtime, e.still_path,
               e.overview, e.season_overview, e.watch_count, e.last_position, e.status, e.completed_date,
               e.air_date, e.is_exact_date, e.is_air_date_manual,
               m.title as show_title, m.backdrop_path, m.poster_path, l.file_path, m.type as media_type,
               (SELECT COUNT(*) FROM Episodes e2 WHERE e2.media_id = m.id AND e2.season_num = e.season_num AND e2.status = 'Unwatched') as season_unwatched_count,
               (SELECT COUNT(*) FROM Episodes e3 WHERE e3.media_id = m.id AND e3.season_num = e.season_num) as season_total_count,
               (SELECT COUNT(*) FROM Episodes e4 WHERE e4.media_id = m.id AND e4.season_num = e.season_num AND e4.status = 'Completed') as season_completed_count,
               (SELECT COUNT(*) FROM Episodes e5 WHERE e5.media_id = m.id AND e5.status = 'Completed') as total_completed_count
        FROM Episodes e
        JOIN Media m ON e.media_id = m.id
        LEFT JOIN Local_Files l ON e.id = l.episode_id
        WHERE e.media_id = ? AND e.status IN ('Watching', 'Unwatched')
          AND (e.air_date IS NULL OR e.air_date = '' OR e.air_date <= date('now', 'localtime'))
        ORDER BY
          CASE WHEN e.status = 'Watching' AND e.last_position > 60 THEN 0 ELSE 1 END,
          e.season_num ASC, e.ep_num ASC
        LIMIT 1
        "
    ).ok()?;

    let mut ep_rows = ep_stmt.query(params![media_id]).ok()?;
    let ep_row = ep_rows.next().ok()??;

    let runtime = ep_row.get::<_, Option<i32>>(5).unwrap_or(Some(0)).unwrap_or(0);
    let last_position = ep_row.get::<_, Option<i32>>(10).unwrap_or(Some(0)).unwrap_or(0);
    let status: String = ep_row.get::<_, Option<String>>(11).unwrap_or_default().unwrap_or_default();
    let season_num: u32 = ep_row.get::<_, u32>(2).unwrap_or(0);
    let ep_num: u32 = ep_row.get::<_, u32>(3).unwrap_or(0);
    let raw_air_date: String = ep_row.get::<_, Option<String>>(13).unwrap_or_default().unwrap_or_default();
    let file_path: Option<String> = ep_row.get::<_, Option<String>>(19).unwrap_or_default();
    let media_type: String = ep_row.get::<_, Option<String>>(20).unwrap_or_default().unwrap_or_default();
    let progress_percentage = crate::sanitizer::calculate_progress_percentage(last_position, runtime);

    let season_unwatched_count: i64 = ep_row.get(21).unwrap_or(0);
    let season_total_count: i64 = ep_row.get(22).unwrap_or(0);
    let season_completed_count: i64 = ep_row.get(23).unwrap_or(0);
    let total_completed_count: i64 = ep_row.get(24).unwrap_or(0);

    let has_local_file = file_path.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
    let is_in_progress = status == "Watching" && last_position > 60 && (runtime == 0 || last_position < (runtime * 60) - 60);
    let is_season_finale = season_num > 0 && season_total_count > 1 && season_unwatched_count == 1 && season_completed_count > 0;

    let is_new_release = if total_completed_count > 0 && !raw_air_date.is_empty() {
        if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&raw_air_date, "%Y-%m-%d") {
            let today = chrono::Local::now().naive_local().date();
            let days_ago = (today - parsed).num_days();
            days_ago >= 0 && days_ago <= 14
        } else {
            false
        }
    } else {
        false
    };

    let has_started_series = total_completed_count > 0 || season_num > 1 || ep_num > 1;

    let (tag, subtitle) = if is_in_progress {
        let mins = last_position / 60;
        ("RESUME WATCHING".to_string(), format!("Paused at {}m • Pick up where you left off", mins))
    } else if is_new_release {
        ("NEW EPISODE RELEASED".to_string(), format!("Season {} Ep {} recently aired and ready", season_num, ep_num))
    } else if is_season_finale {
        ("SEASON FINALE".to_string(), format!("Final episode of Season {}!", season_num))
    } else if has_started_series {
        if has_local_file {
            ("READY TO PLAY".to_string(), format!("Season {} Episode {} ready in your library", season_num, ep_num))
        } else {
            ("UP NEXT".to_string(), format!("Continue Season {} Episode {}", season_num, ep_num))
        }
    } else {
        if media_type == "Movie" {
            ("WATCH MOVIE".to_string(), "Featured from your library".to_string())
        } else {
            ("START SERIES".to_string(), "Begin with Season 1 Episode 1".to_string())
        }
    };

    let ep_id = ep_row.get::<_, i32>(0).unwrap_or(0);
    let raw_still = ep_row.get::<_, Option<String>>(6).unwrap_or_default().unwrap_or_default();
    let still_path = crate::tmdb::resolve_local_still_path(&raw_still, ep_id).unwrap_or(raw_still);

    let raw_backdrop = ep_row.get::<_, Option<String>>(17).unwrap_or_default().unwrap_or_default();
    let backdrop_path = crate::tmdb::resolve_local_backdrop_path(&raw_backdrop, backdrop_size, high_performance_mode).unwrap_or(raw_backdrop);

    let raw_poster = ep_row.get::<_, Option<String>>(18).unwrap_or_default().unwrap_or_default();
    let poster_path = crate::tmdb::resolve_local_poster_path(&raw_poster, poster_size, high_performance_mode).unwrap_or(raw_poster);

    Some(json!({
        "id": ep_id,
        "media_id": ep_row.get::<_, i32>(1).unwrap_or(0),
        "season_num": season_num,
        "ep_num": ep_num,
        "title": ep_row.get::<_, Option<String>>(4).unwrap_or_default().unwrap_or_default(),
        "runtime": runtime,
        "still_path": still_path,
        "overview": ep_row.get::<_, Option<String>>(7).unwrap_or_default().unwrap_or_default(),
        "season_overview": ep_row.get::<_, Option<String>>(8).unwrap_or_default().unwrap_or_default(),
        "watch_count": ep_row.get::<_, Option<i32>>(9).unwrap_or(Some(0)).unwrap_or(0),
        "last_position": last_position,
        "status": status,
        "completed_date": ep_row.get::<_, Option<String>>(12).unwrap_or_default().unwrap_or_default(),
        "air_date": ep_row.get::<_, Option<String>>(13).unwrap_or_default().unwrap_or_default(),
        "is_exact_date": ep_row.get::<_, Option<bool>>(14).unwrap_or_default().unwrap_or(true),
        "is_date_known": true,
        "progress_percentage": progress_percentage,
        "show_title": ep_row.get::<_, Option<String>>(16).unwrap_or_default().unwrap_or_default(),
        "backdrop_path": backdrop_path,
        "poster_path": poster_path,
        "file_path": file_path,
        "media_type": media_type,
        "spotlight_tag": tag,
        "spotlight_subtitle": subtitle,
        "is_season_finale": is_season_finale,
    }))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn get_dashboard_data(
    request_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Value, AppError> {
    tracing::info!("[BACKEND] 🧠 Reading SQLite database to build Dashboard layout...");
    let _permit = state.read_semaphore.acquire().await.unwrap();

    let token = CancellationToken::new();
    {
        let mut tokens = state.cancel_tokens.write().unwrap();
        tokens.insert(request_id.clone(), token.clone());
    }

    let cancel_tokens = state.cancel_tokens.clone();
    let stats_cache = state.stats_cache.clone();

    let mut backdrop_size = "w1280".to_string();
    let mut poster_size = "w500".to_string();
    if let Ok(Some(monitor)) = app.primary_monitor() {
        if monitor.scale_factor() > 1.0 {
            backdrop_size = "original".to_string();
            poster_size = "original".to_string();
        }
    }

    let high_performance_mode = state.settings.read().unwrap().high_performance_mode;

    let result = tokio::task::spawn_blocking(move || {
        handle_panic(std::panic::AssertUnwindSafe(|| {
            let conn = crate::db::get_readonly_connection()?;
            let new_seasons = load_new_seasons(&conn);

        // 1. Algorithmic Hero Spotlight Carousel Selection
        tracing::info!("[BACKEND] 🧠 Running intelligent multi-item Hero Carousel algorithm...");
        let now_ts = chrono::Utc::now().timestamp();
        let mut scored_candidates: Vec<(i64, Value)> = Vec::new();
        let mut seen_media: std::collections::HashSet<i32> = std::collections::HashSet::new();

        if let Ok(mut candidate_stmt) = conn.prepare(
            "
            SELECT m.id,
                   m.type,
                   COALESCE(m.user_rating, 0) as user_rating,
                   (SELECT COUNT(*) FROM Episodes e WHERE e.media_id = m.id AND e.status = 'Completed') as completed_count,
                   (SELECT MAX(h.timestamp) FROM History h JOIN Episodes e ON h.episode_id = e.id WHERE e.media_id = m.id) as last_watched_ts,
                   (SELECT COUNT(*) FROM History h JOIN Episodes e ON h.episode_id = e.id WHERE e.media_id = m.id AND h.timestamp >= strftime('%s', 'now', '-7 days')) as watches_7d,
                   (SELECT COUNT(*) FROM History h JOIN Episodes e ON h.episode_id = e.id WHERE e.media_id = m.id AND h.timestamp >= strftime('%s', 'now', '-14 days')) as watches_14d,
                   (SELECT COUNT(*) FROM Episodes e WHERE e.media_id = m.id AND e.status = 'Watching' AND e.last_position > 60) as in_progress_count
            FROM Media m
            WHERE EXISTS (
                SELECT 1 FROM Episodes e2 
                WHERE e2.media_id = m.id 
                  AND e2.status IN ('Watching', 'Unwatched')
                  AND (e2.air_date IS NULL OR e2.air_date = '' OR e2.air_date <= date('now', 'localtime'))
            )
            ORDER BY
              in_progress_count DESC,
              last_watched_ts DESC NULLS LAST,
              m.id DESC
            LIMIT 50
            "
        ) {
            if let Ok(mut candidate_rows) = candidate_stmt.query([]) {
                while let Ok(Some(row)) = candidate_rows.next() {
                    let m_id: i32 = row.get(0).unwrap_or(0);
                    let user_rating: i32 = row.get(2).unwrap_or(0);
                    let completed_count: i64 = row.get(3).unwrap_or(0);
                    let last_watched_ts: Option<i64> = row.get(4).unwrap_or(None);
                    let watches_7d: i64 = row.get(5).unwrap_or(0);
                    let watches_14d: i64 = row.get(6).unwrap_or(0);

                    if seen_media.insert(m_id) {
                        if let Some(candidate) = resolve_candidate_episode(
                            &conn,
                            m_id,
                            &backdrop_size,
                            &poster_size,
                            high_performance_mode,
                        ) {
                            let tag = candidate["spotlight_tag"].as_str().unwrap_or_default();
                            let has_file = candidate["file_path"].as_str().map(|f| !f.is_empty()).unwrap_or(false);

                            let mut score: i64 = 0;

                            // 1. Paused mid-episode: top priority resume session
                            if tag == "RESUME WATCHING" {
                                score += 200_000;
                            }

                            // 2. Freshly aired episode of tracked show: top priority new drop
                            if tag == "NEW EPISODE RELEASED" {
                                score += 150_000;
                            }

                            // 3. Season finale climax ready to watch
                            if tag == "SEASON FINALE" {
                                score += 100_000;
                            }

                            // 4. Watch recency score
                            if let Some(ts) = last_watched_ts {
                                let diff = now_ts - ts;
                                if diff <= 86400 {
                                    score += 60_000;
                                } else if diff <= 86400 * 3 {
                                    score += 45_000;
                                } else if diff <= 86400 * 7 {
                                    score += 30_000;
                                } else if diff <= 86400 * 14 {
                                    score += 18_000;
                                } else if diff <= 86400 * 30 {
                                    score += 10_000;
                                } else {
                                    score += 3_000;
                                }
                            }

                            // 5. Binge momentum (repeated watches recently)
                            score += watches_7d * 4_000;
                            score += watches_14d * 1_500;

                            // 6. Ready-to-play local file bonus
                            if has_file {
                                score += 8_000;
                            }

                            // 7. Invested series continuity bonus
                            if completed_count > 0 {
                                score += 12_000;
                            }

                            // 8. User rating affinity
                            score += (user_rating as i64) * 800;

                            scored_candidates.push((score, candidate));
                        }
                    }
                }
            }
        }

        // Sort candidates by score descending
        scored_candidates.sort_by(|a, b| b.0.cmp(&a.0));

        let mut hero_eps: Vec<Value> = scored_candidates.into_iter().take(5).map(|(_, v)| v).collect();

        // Library Fallback: If we have fewer than 5 items, fill up to 5 from any remaining library media
        if hero_eps.len() < 5 {
            if let Ok(mut stmt) = conn.prepare("SELECT id FROM Media ORDER BY id DESC LIMIT 20") {
                if let Ok(mut rows) = stmt.query([]) {
                    while let Ok(Some(row)) = rows.next() {
                        let m_id: i32 = row.get(0).unwrap_or(0);
                        if seen_media.insert(m_id) {
                            if let Some(candidate) = resolve_candidate_episode(
                                &conn,
                                m_id,
                                &backdrop_size,
                                &poster_size,
                                high_performance_mode,
                            ) {
                                hero_eps.push(candidate);
                                if hero_eps.len() >= 5 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let hero_ep: Option<Value> = hero_eps.first().cloned();

        // 2. Up Next (Active Shows)
        tracing::info!("[BACKEND] 🧠 Looking for active shows to put in your 'Up Next' queue...");
        let mut cw_stmt = conn.prepare(
            "
            SELECT m.id as media_id
            FROM Media m
            WHERE (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND status = 'Completed' AND season_num > 0) > 0
              AND (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND status = 'Completed' AND season_num > 0) < (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now')))
        ",
        )?;

        let mut cw_eps: Vec<Value> = Vec::new();
        if let Ok(cw_rows) = cw_stmt.query_map([], |row| row.get::<_, i32>(0)) {
            for m_id_res in cw_rows {
                if let Ok(m_id) = m_id_res {
                    if let Some(ref hero) = hero_ep {
                        if hero["media_id"].as_i64().unwrap_or(-1) as i32 == m_id {
                            continue;
                        }
                    }

                    let mut ep_stmt = conn.prepare(
                        "
                        SELECT e.id, e.media_id, e.season_num, e.ep_num, e.title, e.runtime, e.still_path,
                               e.overview, e.season_overview, e.watch_count, e.last_position, e.status, e.completed_date,
                               e.air_date, e.is_exact_date, e.is_air_date_manual,
                               m.title as show_title, m.backdrop_path, m.poster_path, l.file_path, m.type as media_type
                        FROM Episodes e
                        JOIN Media m ON e.media_id = m.id
                        LEFT JOIN Local_Files l ON e.id = l.episode_id
                        WHERE e.media_id = ? AND e.status IN ('Watching', 'Unwatched')
                        ORDER BY e.season_num ASC, e.ep_num ASC
                        LIMIT 1
                    ",
                    )?;

                    let mut ep_rows = ep_stmt.query(params![m_id])?;
                    if let Ok(Some(ep_row)) = ep_rows.next() {
                        let ep_id = ep_row.get::<_, i32>(0).unwrap_or(0);
                        let runtime = ep_row.get::<_, Option<i32>>(5).unwrap_or(Some(0)).unwrap_or(0);
                        let last_position = ep_row.get::<_, Option<i32>>(10).unwrap_or(Some(0)).unwrap_or(0);
                        let progress_percentage = crate::sanitizer::calculate_progress_percentage(last_position, runtime);

                        let raw_air_date: String = ep_row.get::<_, Option<String>>(13).unwrap_or_default().unwrap_or_default();
                        let (sanitized_air_date, is_exact, is_known) = crate::sanitizer::sanitize_date(&raw_air_date);

                        let potential_spoiler = if is_known && !sanitized_air_date.is_empty() {
                            let now = chrono::Utc::now().naive_utc().date();
                            if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&sanitized_air_date, "%Y-%m-%d") {
                                // within last 48 hours or future
                                let duration = now.signed_duration_since(parsed);
                                duration.num_days() <= 2
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        let raw_still_path = ep_row.get::<_, Option<String>>(6).unwrap_or_default().unwrap_or_default();
                        let is_fallback_image = raw_still_path.is_empty();
                        let resolved_still_path = if !is_fallback_image {
                            crate::tmdb::resolve_local_still_path(&raw_still_path, ep_id).unwrap_or(raw_still_path)
                        } else {
                            String::new()
                        };

                        cw_eps.push(json!({
                            "id": ep_id,
                            "media_id": ep_row.get::<_, i32>(1).unwrap_or(0),
                            "season_num": ep_row.get::<_, u32>(2).unwrap_or(0),
                            "ep_num": ep_row.get::<_, u32>(3).unwrap_or(0),
                            "title": ep_row.get::<_, Option<String>>(4).unwrap_or_default().unwrap_or_default(),
                            "runtime": runtime,
                            "still_path": resolved_still_path,
                            "overview": ep_row.get::<_, Option<String>>(7).unwrap_or_default().unwrap_or_default(),
                            "season_overview": ep_row.get::<_, Option<String>>(8).unwrap_or_default().unwrap_or_default(),
                            "watch_count": ep_row.get::<_, Option<i32>>(9).unwrap_or(Some(0)).unwrap_or(0),
                            "last_position": last_position,
                            "status": ep_row.get::<_, Option<String>>(11).unwrap_or_default().unwrap_or_default(),
                            "completed_date": ep_row.get::<_, Option<String>>(12).unwrap_or_default().unwrap_or_default(),
                            "air_date": sanitized_air_date,
                            "is_exact_date": is_exact,
                            "is_date_known": is_known,
                            "progress_percentage": progress_percentage,
                            "is_fallback_image": is_fallback_image,
                            "potential_spoiler": potential_spoiler,

                            "show_title": ep_row.get::<_, Option<String>>(16).unwrap_or_default().unwrap_or_default(),
                            "backdrop_path": (|| {
                                let raw = ep_row.get::<_, Option<String>>(17).unwrap_or_default().unwrap_or_default();
                                crate::tmdb::resolve_local_backdrop_path(&raw, &backdrop_size, high_performance_mode).unwrap_or(raw)
                            })(),
                            "poster_path": (|| {
                                let raw = ep_row.get::<_, Option<String>>(18).unwrap_or_default().unwrap_or_default();
                                crate::tmdb::resolve_local_poster_path(&raw, &poster_size, high_performance_mode).unwrap_or(raw)
                            })(),
                            "file_path": ep_row.get::<_, Option<String>>(19).unwrap_or_default(),
                            "media_type": ep_row.get::<_, Option<String>>(20).unwrap_or_default().unwrap_or_default(),
                        }));
                    }
                }
            }
        }

        // 3. Recently Added
        let mut ra_stmt = conn.prepare(
            "
            SELECT m.*,
                   (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND status = 'Completed' AND season_num > 0) as completed_eps,
                   (SELECT MAX(timestamp) FROM History h JOIN Episodes e ON h.episode_id = e.id WHERE e.media_id = m.id) as last_watched,
                   (SELECT MIN(air_date) FROM Episodes WHERE media_id = m.id AND air_date IS NOT NULL AND air_date != '') as min_year,
                   (SELECT MAX(air_date) FROM Episodes WHERE media_id = m.id AND air_date IS NOT NULL AND air_date != '') as max_year,
                   (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now'))) as total_available
            FROM Media m
            ORDER BY m.id DESC LIMIT 15
        ",
        )?;

        let mut recent_media: Vec<Media> = Vec::new();
        if let Ok(rows) = ra_stmt.query_map([], |row| {
            let m_type: String = row.get(2)?;
            let raw_release_date: String = row.get::<_, Option<String>>(11)?.unwrap_or_default();
            let (sanitized_date, is_exact, is_known) = if raw_release_date.is_empty() && m_type != "TV" {
                ("0000-00-00".to_string(), false, false)
            } else {
                crate::sanitizer::sanitize_date(&raw_release_date)
            };

            let is_unaired = if is_known && !sanitized_date.is_empty() && sanitized_date != "0000-00-00" {
                let now = chrono::Utc::now().naive_utc().date();
                if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&sanitized_date, "%Y-%m-%d") {
                    parsed > now
                } else {
                    false
                }
            } else {
                false
            };

            let raw_synopsis: String = row.get(4)?;
            let sanitized_synopsis = crate::sanitizer::sanitize_text(&raw_synopsis, "No overview available.");

            Ok(Media {
                id: row.get(0)?,
                tmdb_id: row.get(1)?,
                r#type: row.get(2)?,
                title: row.get(3)?,
                synopsis: sanitized_synopsis,
                poster_path: (|| {
                    let raw: Option<String> = row.get(5)?;
                    let p = raw.unwrap_or_default();
                    Ok::<_, rusqlite::Error>(crate::tmdb::resolve_local_poster_path(&p, &poster_size, high_performance_mode).unwrap_or(p))
                })()?,
                backdrop_path: (|| {
                    let raw: String = row.get(6)?;
                    Ok::<_, rusqlite::Error>(crate::tmdb::resolve_local_backdrop_path(&raw, &backdrop_size, high_performance_mode).unwrap_or(raw))
                })()?,
                total_episodes: row.get(7)?,
                status: row.get(8)?,
                vote_average: row.get(9)?,
                user_rating: row.get(10)?,
                release_date: sanitized_date,
                is_exact_date: is_exact,
                is_date_known: is_known,
                genres: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
                networks: row.get::<_, Option<String>>(14)?.unwrap_or_default(),
                is_unaired: is_unaired,
                collection_id: row.get::<_, Option<i32>>(15)?,
                collection_name: row.get::<_, Option<String>>(16)?,
                backdrop_fallback: row.get::<_, Option<String>>(17)?,
                completed_eps: row.get::<_, Option<i32>>(18)?.unwrap_or(0),
                last_watched: row.get::<_, Option<String>>(19)?.unwrap_or_default(),
                min_year: row.get::<_, Option<String>>(20)?.unwrap_or_default(),
                max_year: row.get::<_, Option<String>>(21)?.unwrap_or_default(),
                total_available: Some(row.get::<_, Option<i32>>(22)?.unwrap_or(0)),
                collection_parts: None,
                seasons: Vec::new(),
                episodes: Vec::new(),
                new_season: new_seasons.get(&row.get::<_, i32>(0)?).copied(),
            })
        }) {
            for m in rows.flatten() {
                recent_media.push(m);
            }
        }

        // 4. Stats
        let cached_stats = {
            let cache = stats_cache.read().unwrap();
            cache.clone()
        };

        let stats = if let Some(s) = cached_stats {
            s
        } else {
            let eps_watched: i32 = conn
                .query_row(
                    "SELECT COUNT(*) as count FROM History",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            let hrs_watched: i32 = conn
                .query_row(
                    "SELECT SUM(COALESCE(Episodes.runtime, 0)) FROM Episodes JOIN History ON Episodes.id = History.episode_id",
                    [],
                    |r| r.get::<_, Option<i32>>(0).map(|v| v.unwrap_or(0) / 60),
                )
                .unwrap_or(0);

            let shows_completed: i32 = conn
                .query_row(
                    "SELECT COUNT(*) as c FROM Media WHERE status = 'Completed' OR (SELECT COUNT(*) FROM Episodes WHERE media_id = Media.id AND status = 'Completed' AND season_num > 0) = (SELECT COUNT(*) FROM Episodes WHERE media_id = Media.id AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now')))",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            let avg_rating: f64 = conn
                .query_row(
                    "SELECT AVG(user_rating) FROM Media WHERE user_rating > 0",
                    [],
                    |r| r.get::<_, Option<f64>>(0).map(|v| v.unwrap_or(0.0)),
                )
                .unwrap_or(0.0);

            let new_stats = json!({
                "eps_watched": eps_watched,
                "hrs_watched": hrs_watched,
                "shows_completed": shows_completed,
                "avg_rating": avg_rating
            });

            if let Ok(mut cache) = stats_cache.write() {
                *cache = Some(new_stats.clone());
            }
            new_stats
        };

        Ok(json!({
            "hero_ep": hero_ep,
            "hero_eps": hero_eps,
            "cw_eps": cw_eps,
            "recent_media": recent_media,
            "stats": stats
        }))
        }))
    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

    {
        let mut tokens = cancel_tokens.write().unwrap();
        tokens.remove(&request_id);
    }

    result
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn get_library_data(
    request_id: String,
    media_type: String,
    sort_by: String,
    hide_completed: bool,
    page: Option<u32>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Media>, AppError> {
    tracing::info!("[BACKEND] 🧠 Preparing Library data...");
    let _permit = state.read_semaphore.acquire().await.unwrap();

    let token = CancellationToken::new();
    {
        let mut tokens = state.cancel_tokens.write().unwrap();
        tokens.insert(request_id.clone(), token.clone());
    }

    let cancel_tokens = state.cancel_tokens.clone();
    let _stats_cache = state.stats_cache.clone();

    let mut backdrop_size = "w1280".to_string();
    let mut poster_size = "w500".to_string();
    if let Ok(Some(monitor)) = app.primary_monitor() {
        if monitor.scale_factor() > 1.0 {
            backdrop_size = "original".to_string();
            poster_size = "original".to_string();
        }
    }
    let high_performance_mode = state.settings.read().unwrap().high_performance_mode;

    let result = tokio::task::spawn_blocking(move || {
        handle_panic(std::panic::AssertUnwindSafe(|| {
            tracing::info!("[BACKEND] 🧠 Filtering and sorting library data...");
            let conn = crate::db::get_readonly_connection()?;
            let new_seasons = load_new_seasons(&conn);

            let mut base_query = "
                SELECT m.*,
                       (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND status = 'Completed' AND season_num > 0) as completed_eps,
                       (SELECT MAX(timestamp) FROM History h JOIN Episodes e ON h.episode_id = e.id WHERE e.media_id = m.id) as last_watched,
                       (SELECT MIN(air_date) FROM Episodes WHERE media_id = m.id AND air_date IS NOT NULL AND air_date != '') as min_year,
                       (SELECT MAX(air_date) FROM Episodes WHERE media_id = m.id AND air_date IS NOT NULL AND air_date != '') as max_year,
                       (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now'))) as total_available
                FROM Media m
            ".to_string();

            let mut where_clauses = Vec::new();
            let mut sql_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if media_type != "All" {
                where_clauses.push("m.type = ?".to_string());
                sql_params.push(Box::new(media_type));
            }

            if hide_completed {
                where_clauses.push("(SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND status = 'Completed' AND season_num > 0) < (SELECT COUNT(*) FROM Episodes WHERE media_id = m.id AND season_num > 0 AND (air_date IS NULL OR air_date = '' OR air_date <= date('now'))) ".to_string());
            }

            if !where_clauses.is_empty() {
                base_query.push_str(" WHERE ");
                base_query.push_str(&where_clauses.join(" AND "));
            }

            let order_by = match sort_by.as_str() {
                "Alphabetical (A-Z)" => " ORDER BY m.title ASC",
                "Release Year" => " ORDER BY CASE WHEN m.release_date IS NULL OR m.release_date = '' THEN 1 ELSE 0 END, m.release_date DESC",
                "My Top Rated" => " ORDER BY m.user_rating DESC NULLS LAST, m.title ASC",
                "Sort by Last Watched" => " ORDER BY last_watched DESC NULLS LAST, m.id DESC",
                "Sort by TMDB Rating" => " ORDER BY m.vote_average DESC NULLS LAST, m.title ASC",
                _ => " ORDER BY m.id DESC",
            };

            base_query.push_str(order_by);

            // Apply pagination if provided
            if let Some(p) = page {
                let limit: u32 = 50; // Standard pagination limit
                let offset: u32 = p * limit;
                base_query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));
            }

            let mut stmt = conn.prepare(&base_query)?;

            // Convert Vec<Box<dyn ToSql>> to a format rusqlite understands
            let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params.iter().map(|p| p.as_ref()).collect();

            let mut media_list = Vec::new();
            let mut rows = stmt.query(rusqlite::params_from_iter(param_refs))?;

            let mut batch_count = 0;
            while let Ok(Some(row)) = rows.next() {
                if batch_count % 50 == 0 {
                    if token.is_cancelled() {
                        return Err(AppError::Custom("Task cancelled".to_string()));
                    }
                }
                batch_count += 1;

                let m_type: String = row.get(2)?;
                let raw_release_date: String = row.get::<_, Option<String>>(11)?.unwrap_or_default();
                let (sanitized_date, is_exact, is_known) = if raw_release_date.is_empty() && m_type != "TV" {
                    ("0000-00-00".to_string(), false, false)
                } else {
                    crate::sanitizer::sanitize_date(&raw_release_date)
                };

                let is_unaired = if is_known && !sanitized_date.is_empty() && sanitized_date != "0000-00-00" {
                    let now = chrono::Utc::now().naive_utc().date();
                    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&sanitized_date, "%Y-%m-%d") {
                        parsed > now
                    } else {
                        false
                    }
                } else {
                    false
                };

                let raw_synopsis: String = row.get(4)?;
                let sanitized_synopsis = crate::sanitizer::sanitize_text(&raw_synopsis, "No overview available.");

                media_list.push(Media {
                    id: row.get(0)?,
                    tmdb_id: row.get(1)?,
                    r#type: row.get(2)?,
                    title: row.get(3)?,
                    synopsis: sanitized_synopsis,
                    poster_path: (|| {
                        let raw: Option<String> = row.get(5)?;
                        let p = raw.unwrap_or_default();
                        Ok::<_, rusqlite::Error>(crate::tmdb::resolve_local_poster_path(&p, &poster_size, high_performance_mode).unwrap_or(p))
                    })()?,
                    backdrop_path: (|| {
                        let raw: String = row.get(6)?;
                        Ok::<_, rusqlite::Error>(crate::tmdb::resolve_local_backdrop_path(&raw, &backdrop_size, high_performance_mode).unwrap_or(raw))
                    })()?,
                    total_episodes: row.get(7)?,
                    status: row.get(8)?,
                    vote_average: row.get(9)?,
                    user_rating: row.get(10)?,
                    release_date: sanitized_date,
                    is_exact_date: is_exact,
                    is_date_known: is_known,
                    genres: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
                    networks: row.get::<_, Option<String>>(14)?.unwrap_or_default(),
                    is_unaired: is_unaired,
                    collection_id: row.get::<_, Option<i32>>(15)?,
                    collection_name: row.get::<_, Option<String>>(16)?,
                    backdrop_fallback: row.get::<_, Option<String>>(17)?,
                    completed_eps: row.get::<_, Option<i32>>(18)?.unwrap_or(0),
                    last_watched: row.get::<_, Option<i64>>(19)?.map(|v| v.to_string()).unwrap_or_default(),
                    min_year: row.get::<_, Option<String>>(20)?.unwrap_or_default(),
                    max_year: row.get::<_, Option<String>>(21)?.unwrap_or_default(),
                    total_available: Some(row.get::<_, Option<i32>>(22)?.unwrap_or(0)),
                    collection_parts: None,
                    seasons: Vec::new(),
                    episodes: Vec::new(),
                    new_season: new_seasons.get(&row.get::<_, i32>(0)?).copied(),
                });
            }

            Ok(media_list)
        }))
    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

    {
        let mut tokens = cancel_tokens.write().unwrap();
        tokens.remove(&request_id);
    }

    result
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn clear_unmatched_files() -> Result<(), AppError> {
    handle_panic(|| {
        let conn = get_db_connection()?;
        // Soft Truncation
        conn.execute("DELETE FROM Unmatched_Files", params![])?;
        Ok(())
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn fetch_unmatched_files() -> Result<Vec<UnmatchedFile>, AppError> {
    handle_panic(|| {
        // Must use read_write since it auto-prunes
        let mut conn = get_db_connection()?;
        let mut files = Vec::new();

        // Lightweight exists check
        let mut paths_to_delete = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT file_path, filename, parsed_series, parsed_season, parsed_episode, group_key FROM Unmatched_Files")?;
            let rows = stmt.query_map([], |row| {
                Ok(UnmatchedFile {
                    file_path: row.get(0)?,
                    filename: row.get(1)?,
                    parsed_series: row.get(2)?,
                    parsed_season: row.get(3)?,
                    parsed_episode: row.get(4)?,
                    group_key: row.get(5)?,
                })
            })?;

            for r in rows.flatten() {
                let p = std::path::Path::new(&r.file_path);

                // If it doesn't exist, check root guard to avoid mass deletion on disconnected drive
                if !p.exists() {
                    let mut should_delete = true;

                    // A robust cross-platform way to check drive disconnections is to walk up the path.
                    // If the file is missing, but its parent directory exists, it was deleted.
                    // If the entire parent tree doesn't exist up to the root, the drive is likely unmounted.
                    let mut current_ancestor = p.parent();
                    let mut found_existing_ancestor = false;
                    while let Some(ancestor) = current_ancestor {
                        if ancestor.exists() {
                            found_existing_ancestor = true;
                            break;
                        }
                        current_ancestor = ancestor.parent();
                    }

                    if !found_existing_ancestor {
                        // The entire tree is gone (including the root mount point).
                        // Likely a disconnected drive. Don't prune.
                        should_delete = false;
                    }

                    if should_delete {
                        paths_to_delete.push(r.file_path.clone());
                        continue; // skip adding to returned files
                    }
                }
                files.push(r);
            }
        }

        // Auto-prune missing files
        if !paths_to_delete.is_empty() {
            let tx = conn.transaction()?;
            for p in paths_to_delete {
                let _ = tx.execute(
                    "DELETE FROM Unmatched_Files WHERE file_path = ?",
                    params![p],
                );
            }
            tx.commit()?;
        }

        Ok(files)
    })
}

// ---------------------------------------------------------------------------
// New Season badges
// ---------------------------------------------------------------------------

/// How long after its premiere a season counts as "new".
pub const NEW_SEASON_WINDOW_DAYS: i64 = 30;

/// Shows whose latest new season (season 2 or later) premiered within the last
/// `NEW_SEASON_WINDOW_DAYS` days and has already aired, keyed by media id.
/// A season's premiere is its earliest known episode air date. Uses the local
/// calendar date so "today's" premiere counts on the user's day. Failures are
/// non-fatal: the badge is cosmetic, so an error yields an empty map.
pub(crate) fn load_new_seasons(conn: &rusqlite::Connection) -> HashMap<i32, u32> {
    let mut out = HashMap::new();
    let window = format!("-{} days", NEW_SEASON_WINDOW_DAYS);
    let Ok(mut stmt) = conn.prepare_cached(
        "SELECT media_id, MAX(season_num)
         FROM (
             SELECT media_id, season_num, MIN(air_date) AS premiere
             FROM Episodes
             WHERE season_num > 1
               AND air_date IS NOT NULL AND air_date != '' AND air_date != '0000-00-00'
             GROUP BY media_id, season_num
         )
         WHERE premiere <= date('now', 'localtime')
           AND premiere >= date('now', 'localtime', ?1)
         GROUP BY media_id",
    ) else {
        return out;
    };
    if let Ok(rows) = stmt.query_map(params![window], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, u32>(1)?))
    }) {
        for (media_id, season) in rows.flatten() {
            out.insert(media_id, season);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Watch History
// ---------------------------------------------------------------------------
//
// History is served with keyset (cursor) pagination over (timestamp, id) so
// pages never skip or duplicate rows while new plays are being recorded.
// Rows are grouped into viewing sessions server-side; a page is always closed
// on a session boundary so a single session never gets split across pages.

/// Two plays of the same title belong to the same session if they are less
/// than this many seconds apart (or share a VLC session id).
const HISTORY_SESSION_GAP_SECS: i64 = 6 * 60 * 60;
const HISTORY_DEFAULT_PAGE_SIZE: u32 = 40;
const HISTORY_MAX_PAGE_SIZE: u32 = 200;
/// Extra rows a page may read past its size to finish the session in progress.
const HISTORY_MAX_BLOCK_EXTENSION: u32 = 500;
const HISTORY_COMPLETION_THRESHOLD: f64 = 0.9;
const HISTORY_MAX_DELETE_BATCH: usize = 5000;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct HistoryCursor {
    pub timestamp: i64,
    pub id: i64,
}

struct HistoryRow {
    hist_id: i64,
    timestamp: i64,
    session_id: Option<String>,
    is_legacy: bool,
    is_manual: bool,
    completion_ratio: f64,
    pause_count: i64,
    episode_id: i64,
    season_num: i64,
    ep_num: i64,
    ep_title: String,
    raw_still_path: String,
    raw_air_date: String,
    runtime: i64,
    resume_position: i64,
    media_id: i64,
    show_title: String,
    raw_poster_path: String,
    raw_backdrop_path: String,
    media_type: String,
    file_path: Option<String>,
}

impl HistoryRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let session_id: Option<String> = row.get(2)?;
        Ok(HistoryRow {
            hist_id: row.get(0)?,
            timestamp: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            session_id: session_id.filter(|s| !s.trim().is_empty()),
            is_legacy: row.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0,
            is_manual: row.get::<_, Option<String>>(4)?.map_or(true, |s| s.trim().is_empty()),
            pause_count: row.get::<_, Option<i64>>(5)?.unwrap_or(0).max(0),
            completion_ratio: row
                .get::<_, Option<f64>>(6)?
                .filter(|r| r.is_finite())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
            episode_id: row.get(7)?,
            season_num: row.get::<_, Option<i64>>(8)?.unwrap_or(0).max(0),
            ep_num: row.get::<_, Option<i64>>(9)?.unwrap_or(0).max(0),
            ep_title: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
            raw_still_path: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
            raw_air_date: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
            runtime: row.get::<_, Option<i64>>(13)?.unwrap_or(0).max(0),
            resume_position: row.get::<_, Option<i64>>(14)?.unwrap_or(0).max(0),
            media_id: row.get(15)?,
            show_title: row.get::<_, Option<String>>(16)?.unwrap_or_default(),
            raw_poster_path: row.get::<_, Option<String>>(17)?.unwrap_or_default(),
            raw_backdrop_path: row.get::<_, Option<String>>(18)?.unwrap_or_default(),
            media_type: row.get::<_, Option<String>>(19)?.unwrap_or_default(),
            file_path: row
                .get::<_, Option<String>>(20)?
                .filter(|p| !p.trim().is_empty()),
        })
    }

    /// "archived" (backdated import), "marked" (bulk mark-as-watched),
    /// "completed" (played past the threshold) or "partial".
    fn watch_state(&self) -> &'static str {
        if self.is_legacy {
            "archived"
        } else if self.is_manual {
            "marked"
        } else if self.completion_ratio >= HISTORY_COMPLETION_THRESHOLD {
            "completed"
        } else {
            "partial"
        }
    }

    /// Minutes of this title actually watched in this play.
    fn watched_minutes(&self) -> i64 {
        match self.watch_state() {
            "partial" => (self.runtime as f64 * self.completion_ratio).round() as i64,
            _ => self.runtime,
        }
    }
}

/// Should `next` (older) join the session that currently ends with `prev` (newer)?
fn history_should_group(prev: &HistoryRow, next: &HistoryRow) -> bool {
    if prev.media_id != next.media_id || prev.is_legacy != next.is_legacy {
        return false;
    }
    let same_session = matches!((&prev.session_id, &next.session_id), (Some(a), Some(b)) if a == b);
    if prev.is_legacy {
        // Backdated imports are spread out on purpose; only an explicit shared
        // session id groups them.
        return same_session;
    }
    same_session || (prev.timestamp - next.timestamp).abs() < HISTORY_SESSION_GAP_SECS
}

/// Converts a UTC epoch to the OS-local calendar date key and clock time.
fn history_localize(ts: i64, tz: Option<chrono_tz::Tz>) -> (String, String, bool) {
    let utc = chrono::Utc
        .timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| chrono::Utc.timestamp_opt(0, 0).unwrap());
    let (naive, is_utc) = match tz {
        Some(tz) => (utc.with_timezone(&tz).naive_local(), false),
        None => (utc.naive_utc(), true),
    };
    (
        naive.format("%Y-%m-%d").to_string(),
        naive.format("%-I:%M %p").to_string(),
        is_utc,
    )
}

struct HistoryImageConfig {
    poster_size: String,
    high_performance_mode: bool,
}

fn history_entry_json(
    row: &HistoryRow,
    hist_ids: &[i64],
    play_count: usize,
    tz: Option<chrono_tz::Tz>,
    images: &HistoryImageConfig,
) -> Value {
    let (sanitized_air_date, is_exact, is_known) = crate::sanitizer::sanitize_date(&row.raw_air_date);
    let time_capsule = if is_known {
        calculate_gap(&sanitized_air_date, row.timestamp)
    } else {
        None
    };
    let (date_key, formatted_time, is_utc_fallback) = history_localize(row.timestamp, tz);

    let still_path = if row.raw_still_path.is_empty() {
        String::new()
    } else {
        crate::tmdb::resolve_local_still_path(&row.raw_still_path, row.episode_id as i32)
            .unwrap_or_else(|| row.raw_still_path.clone())
    };

    let state = row.watch_state();
    let progress_percent = match state {
        "partial" => (row.completion_ratio * 100.0).round() as i64,
        _ => 100,
    };

    json!({
        "hist_id": row.hist_id,
        "hist_ids": hist_ids,
        "play_count": play_count,
        "timestamp": row.timestamp,
        "date_key": date_key,
        "formatted_time": formatted_time,
        "is_utc_fallback": is_utc_fallback,
        "is_legacy": row.is_legacy,
        "watch_state": state,
        "progress_percent": progress_percent,
        "pause_count": row.pause_count,
        "resume_position": row.resume_position,
        "episode_id": row.episode_id,
        "season_num": row.season_num,
        "ep_num": row.ep_num,
        "ep_title": row.ep_title,
        "still_path": still_path,
        "air_date": sanitized_air_date,
        "is_date_known": is_known,
        "is_exact_date": is_exact,
        "time_capsule": time_capsule,
        "runtime": row.runtime,
        "media_id": row.media_id,
        "show_title": row.show_title,
        "media_type": row.media_type,
        "file_path": row.file_path,
        "poster_path": crate::tmdb::resolve_local_poster_path(&row.raw_poster_path, &images.poster_size, images.high_performance_mode)
            .unwrap_or_else(|| row.raw_poster_path.clone()),
        "backdrop_path": crate::tmdb::resolve_local_backdrop_path(&row.raw_backdrop_path, "w1280", images.high_performance_mode)
            .unwrap_or_else(|| row.raw_backdrop_path.clone()),
    })
}

/// Collapses a session (newest first) into one card. Repeat plays of the same
/// episode are merged into a single entry that keeps every underlying history id.
fn history_block_json(
    block: &[HistoryRow],
    tz: Option<chrono_tz::Tz>,
    images: &HistoryImageConfig,
) -> Value {
    let mut order: Vec<i64> = Vec::new();
    let mut by_episode: HashMap<i64, (usize, Vec<i64>)> = HashMap::new();
    for (idx, row) in block.iter().enumerate() {
        match by_episode.get_mut(&row.episode_id) {
            Some((_, ids)) => ids.push(row.hist_id),
            None => {
                order.push(row.episode_id);
                by_episode.insert(row.episode_id, (idx, vec![row.hist_id]));
            }
        }
    }

    let mut entries = Vec::with_capacity(order.len());
    let mut watched_minutes = 0i64;
    for episode_id in &order {
        let (idx, ids) = &by_episode[episode_id];
        let row = &block[*idx];
        watched_minutes += row.watched_minutes();
        entries.push(history_entry_json(row, ids, ids.len(), tz, images));
    }

    let newest = &block[0];
    let oldest = &block[block.len() - 1];
    let all_ids: Vec<i64> = block.iter().map(|r| r.hist_id).collect();

    json!({
        "id": format!("h{}", newest.hist_id),
        "kind": if entries.len() > 1 { "session" } else { "single" },
        "media_id": newest.media_id,
        "media_type": newest.media_type,
        "show_title": newest.show_title,
        "is_legacy": newest.is_legacy,
        "date_key": entries[0]["date_key"],
        "start_ts": oldest.timestamp,
        "end_ts": newest.timestamp,
        "watched_minutes": watched_minutes,
        "episode_count": entries.len(),
        "hist_ids": all_ids,
        "entries": entries,
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn fetch_history(
    request_id: String,
    cursor: Option<HistoryCursor>,
    page_size: Option<u32>,
    media_type: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Value, AppError> {
    let media_type_filter = match media_type.as_deref().map(str::trim) {
        None | Some("") | Some("All") => None,
        Some("TV") => Some("TV".to_string()),
        Some("Movie") | Some("Movies") => Some("Movie".to_string()),
        Some(other) => {
            return Err(AppError::Custom(format!(
                "Invalid history filter '{}'. Expected All, TV or Movie.",
                other
            )))
        }
    };
    let limit = page_size
        .unwrap_or(HISTORY_DEFAULT_PAGE_SIZE)
        .clamp(1, HISTORY_MAX_PAGE_SIZE);

    let _permit = state
        .read_semaphore
        .acquire()
        .await
        .map_err(|_| AppError::Custom("History reader is shutting down".to_string()))?;

    let token = CancellationToken::new();
    if let Ok(mut tokens) = state.cancel_tokens.write() {
        tokens.insert(request_id.clone(), token.clone());
    }
    let cancel_tokens = state.cancel_tokens.clone();

    let high_performance_mode = state
        .settings
        .read()
        .map(|s| s.high_performance_mode)
        .unwrap_or(false);
    let poster_size = match app.primary_monitor() {
        Ok(Some(monitor)) if monitor.scale_factor() > 1.0 => "original",
        _ => "w500",
    }
    .to_string();
    let images = HistoryImageConfig {
        poster_size,
        high_performance_mode,
    };

    let result = tokio::task::spawn_blocking(move || {
        handle_panic(std::panic::AssertUnwindSafe(|| {
            let conn = crate::db::get_readonly_connection()?;
            let tz: Option<chrono_tz::Tz> = iana_time_zone::get_timezone()
                .ok()
                .and_then(|name| name.parse().ok());

            let fetch_cap = limit + HISTORY_MAX_BLOCK_EXTENSION;
            let mut stmt = conn.prepare_cached(
                "SELECT h.id, h.timestamp, h.session_id, h.is_legacy, h.start_time, h.pause_count, h.completion_ratio,
                        e.id, e.season_num, e.ep_num, e.title, e.still_path, e.air_date, e.runtime, e.last_position,
                        m.id, m.title, m.poster_path, m.backdrop_path, m.type,
                        (SELECT lf.file_path FROM Local_Files lf WHERE lf.episode_id = e.id LIMIT 1)
                 FROM History h
                 JOIN Episodes e ON h.episode_id = e.id
                 JOIN Media m ON e.media_id = m.id
                 WHERE (?1 IS NULL OR h.timestamp < ?1 OR (h.timestamp = ?1 AND h.id < ?2))
                   AND (?3 IS NULL OR m.type = ?3)
                 ORDER BY h.timestamp DESC, h.id DESC
                 LIMIT ?4",
            )?;

            let mut rows = stmt.query(params![
                cursor.map(|c| c.timestamp),
                cursor.map(|c| c.id).unwrap_or(i64::MAX),
                media_type_filter,
                fetch_cap + 1,
            ])?;

            let mut groups: Vec<Value> = Vec::new();
            let mut block: Vec<HistoryRow> = Vec::new();
            let mut consumed: u32 = 0;
            let mut last_cursor: Option<HistoryCursor> = None;
            let mut has_more = false;

            while let Some(raw) = rows.next()? {
                if consumed % 50 == 0 && token.is_cancelled() {
                    return Err(AppError::Custom("Task cancelled".to_string()));
                }

                let row = match HistoryRow::from_row(raw) {
                    Ok(r) => r,
                    Err(e) => {
                        // One malformed row must never take down the whole timeline.
                        tracing::warn!("[HISTORY] ⚠️ Skipping unreadable history row: {}", e);
                        continue;
                    }
                };

                let joins_block = block
                    .last()
                    .map_or(false, |prev| history_should_group(prev, &row));

                if consumed >= fetch_cap || (consumed >= limit && !joins_block) {
                    has_more = true;
                    break;
                }

                if !joins_block && !block.is_empty() {
                    groups.push(history_block_json(&block, tz, &images));
                    block.clear();
                }

                last_cursor = Some(HistoryCursor {
                    timestamp: row.timestamp,
                    id: row.hist_id,
                });
                block.push(row);
                consumed += 1;
            }

            if !block.is_empty() {
                groups.push(history_block_json(&block, tz, &images));
            }

            Ok(json!({
                "groups": groups,
                "next_cursor": if has_more { last_cursor } else { None },
                "has_more": has_more,
            }))
        }))
    })
    .await
    .unwrap_or_else(|_| Err(AppError::Custom("History task panicked".to_string())));

    if let Ok(mut tokens) = cancel_tokens.write() {
        tokens.remove(&request_id);
    }

    result
}

async fn run_history_delete(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    history_ids: Option<Vec<i64>>,
) -> Result<usize, AppError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<usize, AppError>>();

    state.db_queue.push_high_priority(move |conn| {
        let result = (|| -> Result<usize, AppError> {
            let tx_db = conn.transaction()?;
            let removed = match &history_ids {
                Some(ids) => {
                    let mut removed = 0usize;
                    {
                        let mut stmt = tx_db.prepare_cached("DELETE FROM History WHERE id = ?")?;
                        for id in ids {
                            removed += stmt.execute(params![id])?;
                        }
                    }
                    removed
                }
                None => tx_db.execute("DELETE FROM History", [])?,
            };
            tx_db.commit()?;
            Ok(removed)
        })();
        let _ = tx.send(result);
    });

    let removed = match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(result)) => result?,
        Ok(Err(_)) => return Err(AppError::Custom("Database worker dropped task".to_string())),
        Err(_) => return Err(AppError::Custom("Task Timed Out".to_string())),
    };

    if let Ok(mut cache) = state.stats_cache.write() {
        *cache = None;
    }
    let _ = app.emit("history-changed", json!({ "removed": removed }));
    Ok(removed)
}

/// Removes specific plays from the watch history. Episode watched status and
/// resume positions are left untouched.
#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn delete_history_entries(
    history_ids: Vec<i64>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<usize, AppError> {
    let mut ids: Vec<i64> = history_ids.into_iter().filter(|id| *id > 0).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(0);
    }
    if ids.len() > HISTORY_MAX_DELETE_BATCH {
        return Err(AppError::Custom(format!(
            "Too many history entries in one request ({}). The limit is {}.",
            ids.len(),
            HISTORY_MAX_DELETE_BATCH
        )));
    }
    let removed = run_history_delete(&app, &state, Some(ids)).await?;
    tracing::info!("[HISTORY] 🗑️ Removed {} history entries", removed);
    Ok(removed)
}

/// Clears the entire watch history. Episode watched status is left untouched.
#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn clear_history(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<usize, AppError> {
    let removed = run_history_delete(&app, &state, None).await?;
    tracing::info!("[HISTORY] 🧹 Cleared watch history ({} entries)", removed);
    Ok(removed)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app_handle, state))]
pub async fn run_scan_directory(
    app_handle: tauri::AppHandle,
    directory: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    if !std::fs::metadata(&directory)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        return Err(AppError::Custom(
            "Selected path is not a valid directory.".to_string(),
        ));
    }

    if let Err(e) = std::fs::read_dir(&directory) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return Err(AppError::AccessDenied {
                code: "ACCESS_DENIED".to_string(),
                path: directory,
            });
        }
    }

    if state.is_maintenance_mode.load(Ordering::SeqCst) {
        return Err(AppError::Custom(
            "System Busy: Maintenance mode is currently active.".to_string(),
        ));
    }

    state.is_scan_cancelled.store(false, Ordering::SeqCst);
    state.is_scan_paused.store(false, Ordering::SeqCst);
    let cancel_flag = state.is_scan_cancelled.clone();
    let pause_flag = state.is_scan_paused.clone();

    let supported_extensions = {
        let s = state.settings.read().unwrap();
        s.supported_extensions.clone()
    };

    let _ = app_handle.asset_protocol_scope().allow_directory(&directory, true);

    let task = tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{
                GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
            };
            unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
            }
        }
        handle_panic(std::panic::AssertUnwindSafe(|| {
            let mut conn = get_db_connection()?;
            crate::scanner::scan_directory(
                &directory,
                &mut conn,
                &app_handle,
                cancel_flag,
                pause_flag,
                &supported_extensions,
            )
            .map_err(AppError::from)
        }))
    });

    // Provide a generous timeout for massive directory scans (e.g. 5 minutes)
    match tokio::time::timeout(std::time::Duration::from_secs(300), task).await {
        Ok(res) => res.unwrap_or(Err(AppError::Custom("Task panicked".to_string()))),
        Err(_) => Err(AppError::Custom(
            "Scan Directory Task Timed Out".to_string(),
        )),
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn cancel_active_scan(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    state.is_scan_cancelled.store(true, Ordering::SeqCst);
    tracing::info!("[ACTION] 🛑 Cancel active scan requested by user");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn pause_active_scan(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    state.is_scan_paused.store(true, Ordering::SeqCst);
    tracing::info!("[ACTION] ⏸️ Pause active scan requested by user");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn resume_active_scan(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    state.is_scan_paused.store(false, Ordering::SeqCst);
    tracing::info!("[ACTION] ▶️ Resume active scan requested by user");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app, state))]
pub async fn perform_tmdb_search(
    request_id: String,
    query: String,
    year: Option<String>,
    page: Option<u32>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Value>, AppError> {
    if !state.is_api_authorized.load(Ordering::SeqCst) {
        return Err(AppError::Custom(
            "API Key is invalid or unauthorized.".to_string(),
        ));
    }

    let _permit = state.read_semaphore.acquire().await.unwrap();

    let token = CancellationToken::new();
    {
        let mut tokens = state.cancel_tokens.write().unwrap();
        tokens.insert(request_id.clone(), token.clone());
    }

    let cancel_tokens = state.cancel_tokens.clone();

    let app_handle = app.clone();
    let task = tokio::task::spawn(async move {
        use tauri::Manager;
        let inner_state = app_handle.state::<AppState>();

        if !inner_state.is_api_authorized.load(Ordering::SeqCst) {
            return Err(AppError::Custom(
                "API Key is invalid or unauthorized.".to_string(),
            ));
        }

        let settings = crate::settings::load_settings().map_err(|e| AppError::Custom(e))?;
        if settings.tmdb_api_key.is_empty() {
            return Err(AppError::Custom(
                "Missing TMDB API Key. Please add it in Settings.".to_string(),
            ));
        }

        let api_key = settings.tmdb_api_key;

        let p = page.unwrap_or(1);

        tracing::info!("[NET] 🌐 Sending search query to TMDB API...");

        tokio::select! {
            _ = token.cancelled() => {
                Err(AppError::Custom("Search Task Cancelled".to_string()))
            }
            res = crate::tmdb::search_media(&api_key, &query, year.as_deref(), p) => {
                match res {
                    Ok(results) => Ok(results),
                    Err(e) => {
                        if let AppError::NetworkBlocked = e {
                            inner_state.is_api_authorized.store(false, Ordering::SeqCst);
                            inner_state.db_queue.clear();
                            let _ = app_handle.emit("api-auth-failed", ());
                        }
                        Err(e)
                    }
                }
            }
        }
    });

    let result = match tokio::time::timeout(std::time::Duration::from_secs(15), task).await {
        Ok(res) => res.unwrap_or(Err(AppError::Custom("Task panicked".to_string()))),
        Err(_) => Err(AppError::Custom("Task Timed Out".to_string())),
    };

    {
        let mut tokens = cancel_tokens.write().unwrap();
        tokens.remove(&request_id);
    }

    result
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn get_next_episode_to_play(media_id: i32) -> Result<Option<Value>, AppError> {
    let conn = get_db_connection().map_err(|e| {
        tracing::error!("Failed to get DB connection: {}", e);
        AppError::Custom(e.to_string())
    })?;

    // Find the first unwatched episode in order (regular seasons first, then specials)
    let query = "
        SELECT e.id, e.season_num, e.ep_num, lf.file_path, e.last_position, e.status
        FROM Episodes e
        LEFT JOIN Local_Files lf ON e.id = lf.episode_id
        WHERE e.media_id = ? AND e.status != 'Completed'
        ORDER BY (CASE WHEN e.season_num = 0 THEN 9999 ELSE e.season_num END) ASC, e.ep_num ASC
    ";

    let mut stmt = conn
        .prepare(query)
        .map_err(|e| AppError::Custom(e.to_string()))?;

    let mut rows = stmt
        .query([media_id])
        .map_err(|e| AppError::Custom(e.to_string()))?;

    let mut ideal_missing_ep_num = None;

    while let Some(row) = rows.next().map_err(|e| AppError::Custom(e.to_string()))? {
        let episode_id: i32 = row.get(0).unwrap_or(0);
        let season_num: i32 = row.get(1).unwrap_or(0);
        let ep_num: i32 = row.get(2).unwrap_or(0);
        let file_path: Option<String> = row.get(3).unwrap_or(None);
        let last_position: i32 = match row.get::<_, i64>(4) {
            Ok(v) => v as i32,
            Err(_) => match row.get::<_, f64>(4) {
                Ok(v) => v.round() as i32,
                Err(_) => 0,
            },
        };

        if let Some(path) = file_path {
            let is_gap = ideal_missing_ep_num.is_some();
            return Ok(Some(json!({
                "episode_id": episode_id,
                "season_num": season_num,
                "ep_num": ep_num,
                "file_path": path,
                "last_position": last_position,
                "is_gap": is_gap,
                "missing_ep_num": ideal_missing_ep_num
            })));
        } else {
            if ideal_missing_ep_num.is_none() {
                ideal_missing_ep_num = Some(ep_num);
            }
        }
    }

    // Fallback: If all uncompleted episodes are missing files or if all episodes are Completed (rewatch scenario),
    // check if there is ANY episode with a local file for this media.
    let fallback_query = "
        SELECT e.id, e.season_num, e.ep_num, lf.file_path, e.last_position, e.status
        FROM Episodes e
        INNER JOIN Local_Files lf ON e.id = lf.episode_id
        WHERE e.media_id = ?
        ORDER BY (CASE WHEN e.season_num = 0 THEN 9999 ELSE e.season_num END) ASC, e.ep_num ASC
        LIMIT 1
    ";
    let mut fallback_stmt = conn
        .prepare(fallback_query)
        .map_err(|e| AppError::Custom(e.to_string()))?;
    let mut fallback_rows = fallback_stmt
        .query([media_id])
        .map_err(|e| AppError::Custom(e.to_string()))?;

    if let Some(row) = fallback_rows.next().map_err(|e| AppError::Custom(e.to_string()))? {
        let episode_id: i32 = row.get(0).unwrap_or(0);
        let season_num: i32 = row.get(1).unwrap_or(0);
        let ep_num: i32 = row.get(2).unwrap_or(0);
        let file_path: Option<String> = row.get(3).unwrap_or(None);
        let last_position: i32 = match row.get::<_, i64>(4) {
            Ok(v) => v as i32,
            Err(_) => match row.get::<_, f64>(4) {
                Ok(v) => v.round() as i32,
                Err(_) => 0,
            },
        };
        if let Some(path) = file_path {
            return Ok(Some(json!({
                "episode_id": episode_id,
                "season_num": season_num,
                "ep_num": ep_num,
                "file_path": path,
                "last_position": last_position,
                "is_gap": false,
                "missing_ep_num": None::<i32>
            })));
        }
    }

    Ok(None)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app_handle, state))]
pub async fn play_next_episode_cmd(
    media_id: i32,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let next_ep_info = get_next_episode_to_play(media_id).await?;
    if let Some(info) = next_ep_info {
        let episode_id = info["episode_id"]
            .as_i64()
            .ok_or_else(|| AppError::Custom("Invalid episode ID".to_string()))? as i32;
        let file_path = info["file_path"]
            .as_str()
            .ok_or_else(|| AppError::Custom("No local video file found for this episode".to_string()))?
            .to_string();
        let last_position = info["last_position"].clone();

        crate::vlc::play_episode_cmd(app_handle, episode_id, file_path, last_position, state).await
    } else {
        Err(AppError::Custom(
            "No playable episodes or linked local files found. Please link a video file first.".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn assign_unmatched_to_tracker(
    request_id: String,
    tmdb_id: String,
    media_type: String,
    group_key: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    if !state.is_api_authorized.load(Ordering::SeqCst) {
        return Err(AppError::Custom(
            "API Key is invalid or unauthorized.".to_string(),
        ));
    }

    let cancel_tokens = state.cancel_tokens.clone();

    let settings = crate::settings::load_settings().map_err(|e| AppError::Custom(e))?;
    if settings.tmdb_api_key.is_empty() {
        return Err(AppError::Custom(
            "Missing TMDB API Key. Please add it in Settings.".to_string(),
        ));
    }

    tracing::info!("[BACKEND] 🧠 Fetching TMDB metadata to resolve unmatched local files...");
    let valid_media_type = crate::models::MediaType::from_str(&media_type)
        .as_str()
        .to_string();
    let details_res: Result<Value, AppError> =
        match crate::tmdb::get_media_details(&settings.tmdb_api_key, &tmdb_id, &valid_media_type)
            .await
        {
            Ok(res) => Ok(res),
            Err(e) => {
                if let AppError::NetworkBlocked = e {
                    state.is_api_authorized.store(false, Ordering::SeqCst);
                    state.db_queue.clear();
                    let _ = app.emit("api-auth-failed", ());
                    return Err(AppError::Custom("API Key was revoked.".to_string()));
                }
                return Err(e);
            }
        };

    let api_key = settings.tmdb_api_key.clone();
    let media_type_clone = valid_media_type.clone();
    let tmdb_id_clone = tmdb_id.clone();
    let app_clone_for_task = app.clone();

    // Detect High-DPI displays dynamically for backdrop high-resolution support
    let mut backdrop_size = "w1280".to_string();
    let mut poster_size = "w500".to_string();
    if let Ok(Some(monitor)) = app.primary_monitor() {
        if monitor.scale_factor() > 1.0 {
            backdrop_size = "original".to_string();
            poster_size = "original".to_string();
        }
    }
    let image_config = crate::tmdb::ImageConfig {
        backdrop_size,
        poster_size,
    };

    let task = tokio::task::spawn(async move {
        use tauri::Manager;
        let state = app_clone_for_task.state::<AppState>();
        let high_performance_mode = state.settings.read().unwrap().high_performance_mode;
        let failed_syncs_clone = state.failed_image_syncs.clone();
        let cancel_tokens_clone = state.cancel_tokens.clone();

        // Fetch files for this group before spawning the thread
        let unmatched_files = tokio::task::spawn_blocking({
            let group_key = group_key.clone();
            move || {
            let mut unmatched_files = Vec::new();
            if let Ok(conn) = get_db_connection() {
                if let Ok(mut stmt) = conn.prepare("SELECT file_path, parsed_season, parsed_episode FROM Unmatched_Files WHERE group_key = ?") {
                    if let Ok(rows) = stmt.query_map(params![group_key], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<i32>>(1)?,
                            row.get::<_, Option<i32>>(2)?,
                        ))
                    }) {
                        for r in rows.flatten() {
                            unmatched_files.push(r);
                        }
                    }
                }
            }
            unmatched_files
            }
        }).await.unwrap_or_default();

        let media_id_res = tokio::task::spawn_blocking({
            let details = match &details_res {
                Ok(v) => v.clone(),
                Err(_) => serde_json::Value::Null,
            };
            let valid_media_type = valid_media_type.clone();
            let tmdb_id_clone = tmdb_id_clone.clone();
            let failed_syncs_clone = failed_syncs_clone.clone();
            move || {
                handle_panic(std::panic::AssertUnwindSafe(|| {
                    if !details.is_null() {

                    let mut synopsis = details["synopsis"].as_str().unwrap_or("").to_string();
            if synopsis.chars().count() > 10000 {
                synopsis = synopsis.chars().take(10000).collect::<String>();
                synopsis.push_str("...");
            }

            if let Some(poster) = details["poster_path"].as_str() {
                let poster_str = poster.to_string();
                if !poster_str.is_empty() {
                    let failed_queue = failed_syncs_clone.clone();
                    let size = image_config.poster_size.clone();
                    tokio::spawn(async move {
                        if crate::tmdb::download_image(&poster_str, &size, high_performance_mode).await.is_none() {
                            if let Ok(mut queue) = failed_queue.write() {
                                queue.insert((poster_str, size.clone()));
                            }
                        }
                    });
                }
            }
            if let Some(backdrop) = details["backdrop_path"].as_str() {
                let backdrop_str = backdrop.to_string();
                if !backdrop_str.is_empty() {
                    let failed_queue = failed_syncs_clone.clone();
                    let size = image_config.backdrop_size.clone();
                    tokio::spawn(async move {
                        if crate::tmdb::download_image(&backdrop_str, &size, high_performance_mode).await.is_none() {
                            if let Ok(mut queue) = failed_queue.write() {
                                queue.insert((backdrop_str, size.clone()));
                            }
                        }
                    });
                }
            }

            if let Ok(mut conn) = get_db_connection() {
                if let Ok(tx) = conn.transaction() {
                    let mut existing_id = None;
                    {
                        if let Ok(mut stmt) = tx.prepare("SELECT id FROM Media WHERE tmdb_id=?") {
                            if let Ok(mut rows) = stmt.query(params![tmdb_id]) {
                                if let Ok(Some(row)) = rows.next() {
                                    existing_id = Some(row.get::<_, i32>(0).unwrap_or(0));
                                }
                            }
                        }
                    }

                    let media_id = if let Some(id) = existing_id {
                        let _ = tx.execute(
                                    "UPDATE Media SET \"title\" = ?, synopsis = ?, poster_path = ?, backdrop_path = ?, total_episodes = ?, vote_average = ?, release_date = ?, is_exact_date = ?, collection_id = ?, collection_name = ?, backdrop_fallback = ?
                             WHERE id = ?",
                            params![
                                details["title"].as_str().unwrap_or("Unknown Title"),
                                synopsis,
                                        if details["poster_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["poster_path"].as_str().unwrap_or("")) },
                                        if details["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["backdrop_path"].as_str().unwrap_or("")) },
                                details["total_episodes"].as_i64().unwrap_or(1) as i32,
                                details["vote_average"].as_f64().unwrap_or(0.0),
                                if details["release_date"].as_str().unwrap_or("") == "0000-00-00" { None } else { Some(details["release_date"].as_str().unwrap_or("")) },
                                details["is_exact_date"].as_bool().unwrap_or(true),
                                details["collection_id"].as_i64().map(|id| id as i32),
                                details["collection_name"].as_str(),
                                        if details["fallback_type"].as_str().unwrap_or("").is_empty() { None } else { Some(details["fallback_type"].as_str().unwrap_or("")) },
                                id
                            ]
                        );
                        id
                    } else {
                        let _ = tx.execute(
                                    "INSERT INTO Media (tmdb_id, \"type\", \"title\", synopsis, poster_path, backdrop_path, total_episodes, status, vote_average, release_date, is_exact_date, collection_id, collection_name, backdrop_fallback)
                                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                             ON CONFLICT(tmdb_id, \"type\") DO UPDATE SET
                                \"title\"=excluded.\"title\", synopsis=excluded.synopsis, poster_path=excluded.poster_path,
                                backdrop_path=excluded.backdrop_path, total_episodes=excluded.total_episodes,
                                vote_average=excluded.vote_average, release_date=excluded.release_date, is_exact_date=excluded.is_exact_date,
                                        collection_id=excluded.collection_id, collection_name=excluded.collection_name, backdrop_fallback=excluded.backdrop_fallback",
                            params![
                                tmdb_id,
                                valid_media_type,
                                details["title"].as_str().unwrap_or("Unknown Title"),
                                synopsis,
                                        if details["poster_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["poster_path"].as_str().unwrap_or("")) },
                                        if details["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(details["backdrop_path"].as_str().unwrap_or("")) },
                                details["total_episodes"].as_i64().unwrap_or(1) as i32,
                                details["status"].as_str().unwrap_or("Plan to Watch"),
                                details["vote_average"].as_f64().unwrap_or(0.0),
                                if details["release_date"].as_str().unwrap_or("") == "0000-00-00" { None } else { Some(details["release_date"].as_str().unwrap_or("")) },
                                details["is_exact_date"].as_bool().unwrap_or(true),
                                details["collection_id"].as_i64().map(|id| id as i32),
                                        details["collection_name"].as_str(),
                                        if details["fallback_type"].as_str().unwrap_or("").is_empty() { None } else { Some(details["fallback_type"].as_str().unwrap_or("")) }
                            ]
                        );

                        let mut new_id = tx.last_insert_rowid() as i32;
                        if new_id == 0 {
                            if let Ok(mut stmt) = tx.prepare("SELECT id FROM Media WHERE tmdb_id=? AND \"type\"=?") {
                                if let Ok(mut rows) = stmt.query(params![tmdb_id_clone, valid_media_type]) {
                                    if let Ok(Some(row)) = rows.next() {
                                        new_id = row.get::<_, i32>(0).unwrap_or(0);
                                    }
                                }
                            }
                        }
                        new_id
                    };
                            let _ = tx.commit();
                            Ok::<i32, AppError>(media_id)
                        } else {
                            Err(AppError::Custom("DB transaction error".to_string()))
                        }
                    } else {
                        Err(AppError::Custom("DB connection error".to_string()))
                    }
                    } else {
                        Err(AppError::Custom("No details".to_string()))
                    }
                }))
            }
        }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())))?;

        let media_id = media_id_res;

        let token = CancellationToken::new();
        {
            let mut tokens = cancel_tokens_clone.write().unwrap();
            tokens.insert(media_id.to_string(), token.clone());
        }

        let ep_status = "Unwatched";
        let ep_watch_count = 0;

        if media_type_clone == "TV" {
            // Iterative season fetching and inserting
            if let Ok(details) = details_res {
                if let Some(seasons) = details["seasons"].as_array() {
                    let total_seasons = seasons.len();
                    for (index, season) in seasons.iter().enumerate() {
                        if let Some(s_num) = season["season_number"].as_i64() {
                            if s_num >= 0 {
                                if token.is_cancelled() {
                                    tracing::info!("[IPC: CANCEL] 🛑 Fetch loop cancelled for unmatched media_id {}", media_id);
                                    break;
                                }

                                crate::commands::check_rate_limit(
                                    &app_clone_for_task.state::<AppState>(),
                                )
                                .await;

                                tokio::task::yield_now().await;

                                if index > 0 {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100))
                                        .await;
                                }

                                let _ = app_clone_for_task.emit(
                                    "sync-progress",
                                    json!({
                                        "mediaId": media_id,
                                        "tmdbId": tmdb_id_clone,
                                        "currentSeason": index + 1,
                                        "totalSeasons": total_seasons
                                    }),
                                );

                                match crate::tmdb::get_tv_season_episodes(
                                    &api_key,
                                    &tmdb_id_clone,
                                    s_num as u32,
                                )
                                .await
                                {
                                    Ok(eps) => {
                                        let res = tokio::task::spawn_blocking(move || {
                                        handle_panic(|| {
                                            let mut inserted_eps = Vec::new();
                                            if let Ok(mut conn) = get_db_connection() {
                                                if let Ok(tx) = conn.transaction() {
                                                    for ep in eps {
                                                        let mut ep_overview = ep["overview"].as_str().unwrap_or("").to_string();
                                                        if ep_overview.chars().count() > 10000 {
                                                            ep_overview = ep_overview.chars().take(10000).collect::<String>();
                                                            ep_overview.push_str("...");
                                                        }
                                                        let mut season_overview = ep["season_overview"].as_str().unwrap_or("").to_string();
                                                        if season_overview.chars().count() > 10000 {
                                                            season_overview = season_overview.chars().take(10000).collect::<String>();
                                                            season_overview.push_str("...");
                                                        }
                                                        let season_num = ep["season_num"].as_i64().unwrap_or(1) as u32;
                                                        let ep_num = ep["ep_num"].as_i64().unwrap_or(1) as u32;
                                                        let mut should_update_air_date = true;
                                                        if let Ok(mut stmt) = tx.prepare("SELECT is_air_date_manual FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                            if let Ok(mut rows) = stmt.query(params![media_id, season_num, ep_num]) {
                                                                if let Ok(Some(row)) = rows.next() {
                                                                    let is_manual: bool = row.get(0).unwrap_or(false);
                                                                    if is_manual {
                                                                        should_update_air_date = false;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        let new_air_date = ep["air_date"].as_str().unwrap_or("");
                                                        let valid_air_date = if new_air_date.to_uppercase() == "TBD" || (new_air_date.len() > 0 && NaiveDate::parse_from_str(new_air_date, "%Y-%m-%d").is_err() && new_air_date.len() != 4) {
                                                            should_update_air_date = false;
                                                            ""
                                                        } else {
                                                            new_air_date
                                                        };
                                                        if should_update_air_date {
                                                            let _ = tx.execute(
                                                                "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count, air_date, is_exact_date)
                                                                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                                 ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                    \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                    overview=excluded.overview, season_overview=excluded.season_overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                                                params![
                                                                    media_id,
                                                                    season_num,
                                                                    ep_num,
                                                                    ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                    ep["runtime"].as_i64().unwrap_or(0) as i32,
                                                                if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) },
                                                                    ep_overview,
                                                                    season_overview,
                                                                    ep_status,
                                                                    ep_watch_count,
                                                                    valid_air_date,
                                                                    ep["is_exact_date"].as_bool().unwrap_or(true)
                                                                ]
                                                            );
                                                        } else {
                                                            let _ = tx.execute(
                                                                "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count)
                                                                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                                 ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                    \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                    overview=excluded.overview, season_overview=excluded.season_overview",
                                                                params![
                                                                    media_id,
                                                                    season_num,
                                                                    ep_num,
                                                                    ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                    ep["runtime"].as_i64().unwrap_or(0) as i32,
                                                                if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) },
                                                                    ep_overview,
                                                                    season_overview,
                                                                    ep_status,
                                                                    ep_watch_count
                                                                ]
                                                            );
                                                        }

                                                        if let Some(path) = if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) } {
                                                            if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                                if let Ok(mut rows) = stmt.query(params![media_id, season_num, ep_num]) {
                                                                    if let Ok(Some(row)) = rows.next() {
                                                                        let ep_id: i32 = row.get(0).unwrap_or(0);
                                                                        inserted_eps.push((ep_id, path.to_string()));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    let _ = tx.commit();
                                                }
                                            }
                                            Ok::<Vec<(i32, String)>, AppError>(inserted_eps)
                                        })
                                    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

                                        if let Ok(inserted_eps) = res {
                                            if !inserted_eps.is_empty() {
                                                // "Lazy Discovery" architecture: Priority active-season vs background idle downloading
                                                let is_active_season = index == 0;
                                                let token_clone = token.clone();

                                                let download_future = async move {
                                                    for chunk in inserted_eps.chunks(20) {
                                                        if token_clone.is_cancelled() {
                                                            break;
                                                        }

                                                        let mut tasks = Vec::new();
                                                        for (ep_id, path) in chunk {
                                                            let path_clone = path.clone();
                                                            let ep_id_clone = *ep_id;
                                                            tasks.push(tokio::spawn(async move {
                                                            crate::tmdb::download_episode_still(&path_clone, ep_id_clone, high_performance_mode).await;
                                                        }));
                                                        }

                                                        for t in tasks {
                                                            let _ = t.await;
                                                        }

                                                        if token_clone.is_cancelled() {
                                                            break;
                                                        }
                                                    }
                                                };

                                                if is_active_season {
                                                    // Await inline for the active season
                                                    download_future.await;
                                                } else {
                                                    // Spawn background task for idle seasons
                                                    tokio::spawn(download_future);
                                                }
                                            }
                                        }
                                    }
                                    Err(AppError::Custom(err)) if err == "NOT_FOUND" => {
                                        tracing::warn!("[API] ⚠️ Season {} missing (404) for show ID {}. Skipping.", s_num, tmdb_id_clone);
                                        let _ = app_clone_for_task.emit(
                                            "warning-toast",
                                            format!("Season {} missing from TMDB, skipped.", s_num),
                                        );
                                        continue;
                                    }
                                    Err(AppError::Custom(err))
                                        if err.starts_with("RATE_LIMIT:") =>
                                    {
                                        let parts: Vec<&str> = err.split(':').collect();
                                        let retry_after = parts
                                            .get(1)
                                            .unwrap_or(&"1")
                                            .parse::<u64>()
                                            .unwrap_or(1);
                                        let async_state = app_clone_for_task.state::<AppState>();
                                        async_state.is_rate_limited.store(true, Ordering::SeqCst);
                                        async_state.rate_limit_reset.store(
                                            chrono::Utc::now().timestamp() + retry_after as i64,
                                            Ordering::SeqCst,
                                        );

                                        tracing::warn!(
                                            "[API] ⚠️ Rate limited. Pausing queue for {} seconds.",
                                            retry_after
                                        );
                                        tokio::time::sleep(tokio::time::Duration::from_secs(
                                            retry_after,
                                        ))
                                        .await;
                                        app_clone_for_task
                                            .state::<AppState>()
                                            .is_rate_limited
                                            .store(false, Ordering::SeqCst);

                                        // Retry once directly inline after sleeping
                                        if let Ok(eps) = crate::tmdb::get_tv_season_episodes(
                                            &api_key,
                                            &tmdb_id_clone,
                                            s_num as u32,
                                        )
                                        .await
                                        {
                                            let res = tokio::task::spawn_blocking({
                                                let media_id = media_id;
                                                let ep_status = ep_status;
                                                let ep_watch_count = ep_watch_count;
                                                move || {
                                                    handle_panic(|| {
                                                        let mut inserted_eps = Vec::new();
                                                        if let Ok(mut conn) = get_db_connection() {
                                                            if let Ok(tx) = conn.transaction() {
                                                                for ep in eps {
                                                                    // Simplified retry logic
                                                                    let ep_num = ep["ep_num"].as_i64().unwrap_or(1) as u32;
                                                                    let raw_still_path = if ep["still_path"].as_str().unwrap_or("").is_empty() { None } else { Some(ep["still_path"].as_str().unwrap_or("")) };
                                                                    let _ = tx.execute(
                                                                        "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, season_overview, status, watch_count, air_date, is_exact_date)
                                                                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                                                         ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                                                            \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                                                            overview=excluded.overview, season_overview=excluded.season_overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                                                        params![
                                                                            media_id,
                                                                            s_num,
                                                                            ep_num,
                                                                            ep["title"].as_str().unwrap_or("Unknown Title"),
                                                                            ep["runtime"].as_i64().unwrap_or(0) as i32,
                                                                            raw_still_path,
                                                                            ep["overview"].as_str().unwrap_or(""),
                                                                            ep["season_overview"].as_str().unwrap_or(""),
                                                                            ep_status,
                                                                            ep_watch_count,
                                                                            ep["air_date"].as_str().unwrap_or(""),
                                                                            ep["is_exact_date"].as_bool().unwrap_or(true)
                                                                        ]
                                                                    );
                                                                    if let Some(path) = raw_still_path {
                                                                        if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id=? AND season_num=? AND ep_num=?") {
                                                                            if let Ok(mut rows) = stmt.query(params![media_id, s_num, ep_num]) {
                                                                                if let Ok(Some(row)) = rows.next() {
                                                                                    let ep_id: i32 = row.get(0).unwrap_or(0);
                                                                                    inserted_eps.push((ep_id, path.to_string()));
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                let _ = tx.commit();
                                                            }
                                                        }
                                                        Ok::<Vec<(i32, String)>, AppError>(inserted_eps)
                                                    })
                                                }
                                            }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

                                            if let Ok(inserted_eps) = res {
                                                if !inserted_eps.is_empty() {
                                                    let _is_active_season = true; // In retry block, just process it since it's a fallback
                                                    let token_clone = token.clone();

                                                    let download_future = async move {
                                                        let mut tasks = Vec::new();

                                                        for (ep_id, path) in inserted_eps {
                                                            if token_clone.is_cancelled() {
                                                                break;
                                                            }

                                                            let permit =
                                                                crate::tmdb::GLOBAL_IMAGE_SEMAPHORE
                                                                    .acquire()
                                                                    .await
                                                                    .unwrap();
                                                            let path_clone = path.clone();
                                                            let ep_id_clone = ep_id;

                                                            tasks.push(tokio::spawn(async move {
                                                                crate::tmdb::download_episode_still(&path_clone, ep_id_clone, high_performance_mode).await;
                                                                drop(permit);
                                                            }));
                                                        }

                                                        for t in tasks {
                                                            let _ = t.await;
                                                        }
                                                    };
                                                    tokio::spawn(download_future);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "[API] 🚨 Error fetching season {}: {}",
                                            s_num,
                                            e
                                        );
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    let _ = app_clone_for_task.emit(
                        "sync-progress",
                        json!({
                            "mediaId": media_id,
                            "tmdbId": tmdb_id_clone,
                            "currentSeason": total_seasons + 1,
                            "totalSeasons": total_seasons
                        }),
                    );
                }
            }
        } else {
            if !token.is_cancelled() {
                let details_clone = match &details_res {
                    Ok(v) => v.clone(),
                    Err(_) => serde_json::Value::Null,
                };
                let inner_clone = details_clone.clone();
                let _ = tokio::task::spawn_blocking(move || {
                handle_panic(|| {
                    if let Ok(mut conn) = get_db_connection() {
                        if let Ok(tx) = conn.transaction() {
                            let mut should_update_air_date = true;
                            if let Ok(mut stmt) = tx.prepare("SELECT is_air_date_manual FROM Episodes WHERE media_id=? AND season_num=1 AND ep_num=1") {
                                if let Ok(mut rows) = stmt.query(params![media_id]) {
                                    if let Ok(Some(row)) = rows.next() {
                                        let is_manual: bool = row.get(0).unwrap_or(false);
                                        if is_manual {
                                            should_update_air_date = false;
                                        }
                                    }
                                }
                            }
                            let new_air_date = inner_clone["release_date"].as_str().unwrap_or("");
                            let valid_air_date = if new_air_date.to_uppercase() == "TBD" || (new_air_date.len() > 0 && NaiveDate::parse_from_str(new_air_date, "%Y-%m-%d").is_err() && new_air_date.len() != 4) {
                                should_update_air_date = false;
                                ""
                            } else {
                                new_air_date
                            };
                            let mut synopsis = inner_clone["synopsis"].as_str().unwrap_or("").to_string();
                            if synopsis.chars().count() > 10000 {
                                synopsis = synopsis.chars().take(10000).collect::<String>();
                                synopsis.push_str("...");
                            }
                            if should_update_air_date {
                                let _ = tx.execute(
                                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, status, watch_count, air_date, is_exact_date)
                                     VALUES (?, 1, 1, ?, ?, ?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                        \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                        overview=excluded.overview, air_date=excluded.air_date, is_exact_date=excluded.is_exact_date",
                                    params![
                                        media_id,
                                        inner_clone["title"].as_str().unwrap_or("Unknown Title"),
                                        inner_clone.get("runtime").and_then(|r| r.as_i64()).unwrap_or(0) as i32,
                                        if inner_clone["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(inner_clone["backdrop_path"].as_str().unwrap_or("")) },
                                        synopsis,
                                        ep_status,
                                        ep_watch_count,
                                        valid_air_date,
                                        inner_clone["is_exact_date"].as_bool().unwrap_or(true)
                                    ]
                                );
                            } else {
                                let _ = tx.execute(
                                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, status, watch_count)
                                     VALUES (?, 1, 1, ?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                                        \"title\"=excluded.\"title\", runtime=excluded.runtime, still_path=excluded.still_path,
                                        overview=excluded.overview",
                                    params![
                                        media_id,
                                        inner_clone["title"].as_str().unwrap_or("Unknown Title"),
                                        inner_clone.get("runtime").and_then(|r| r.as_i64()).unwrap_or(0) as i32,
                                        if inner_clone["backdrop_path"].as_str().unwrap_or("").is_empty() { None } else { Some(inner_clone["backdrop_path"].as_str().unwrap_or("")) },
                                        synopsis,
                                        ep_status,
                                        ep_watch_count
                                    ]
                                );
                            }
                            let _ = tx.commit();
                        }
                    }
                    Ok::<(), AppError>(())
                })
        }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())))?;

                if let Some(c_id) = details_clone.get("collection_id").and_then(|v| v.as_i64()) {
                    tokio::task::yield_now().await;
                    if let Ok(col) =
                        crate::tmdb::get_collection_details(&api_key, c_id as i32).await
                    {
                        let _ = tokio::task::spawn_blocking(move || {
                        handle_panic(|| {
                                if let Ok(conn) = get_db_connection() {
                                let parts_str = col.get("parts").map(|p| p.to_string()).unwrap_or_else(|| "[]".to_string());
                                let _ = conn.execute(
                                    "INSERT INTO Collections (id, name, overview, poster_path, backdrop_path, parts)
                                     VALUES (?, ?, ?, ?, ?, ?)
                                     ON CONFLICT(id) DO UPDATE SET
                                     name=excluded.name, overview=excluded.overview, poster_path=excluded.poster_path, backdrop_path=excluded.backdrop_path, parts=excluded.parts",
                                    params![
                                        c_id as i32,
                                        col.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown Collection"),
                                        crate::sanitizer::sanitize_text(col.get("overview").and_then(|v| v.as_str()).unwrap_or(""), "No overview available."),
                                        col.get("poster_path").and_then(|v| v.as_str()).unwrap_or(""),
                                        col.get("backdrop_path").and_then(|v| v.as_str()).unwrap_or(""),
                                        parts_str
                                    ]
                                );
                            }
                            Ok::<(), AppError>(())
                        })
                    }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));
                    }
                }
            }
        }

        // Now assign the unmatched files
        let _ = tokio::task::spawn_blocking(move || {
            handle_panic(|| {
                if let Ok(mut conn) = get_db_connection() {
                    if let Ok(tx) = conn.transaction() {
                        for (file_path, parsed_season, parsed_ep) in &unmatched_files {
                            let mut matched_ep_id = None;
                            if valid_media_type == "TV" {
                                if let (Some(s_num), Some(e_num)) = (*parsed_season, *parsed_ep) {
                                    if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id = ? AND season_num = ? AND ep_num = ?") {
                                        if let Ok(mut rows) = stmt.query(params![media_id, s_num, e_num]) {
                                            if let Ok(Some(row)) = rows.next() {
                                                matched_ep_id = Some(row.get::<_, i32>(0).unwrap_or(0));
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Movie, assume season 1 episode 1
                                if let Ok(mut stmt) = tx.prepare("SELECT id FROM Episodes WHERE media_id = ? AND season_num = 1 AND ep_num = 1") {
                                    if let Ok(mut rows) = stmt.query(params![media_id]) {
                                        if let Ok(Some(row)) = rows.next() {
                                            matched_ep_id = Some(row.get::<_, i32>(0).unwrap_or(0));
                                        }
                                    }
                                }
                            }

                            if let Some(ep_id) = matched_ep_id {
                                let _ = tx.execute(
                                    "INSERT INTO Local_Files (episode_id, file_path) VALUES (?, ?) ON CONFLICT(episode_id) DO UPDATE SET file_path=excluded.file_path",
                                    params![ep_id, file_path],
                                );
                                // Remove this unmatched file
                                let _ = tx.execute("DELETE FROM Unmatched_Files WHERE file_path = ?", params![file_path]);
                            }
                        }

                        let _ = tx.commit();
                    }
                }
                Ok::<(), AppError>(())
            })
        }).await.unwrap_or(Err(AppError::Custom("Task panicked".to_string())));

        {
            let mut tokens = cancel_tokens_clone.write().unwrap();
            tokens.remove(&media_id.to_string());
        }

        Ok::<(), AppError>(())
    });

    let result = match tokio::time::timeout(std::time::Duration::from_secs(15), task).await {
        Ok(res) => res.unwrap_or(Err(AppError::Custom("Task panicked".to_string()))),
        Err(_) => Err(AppError::Custom("Task Timed Out".to_string())),
    };

    {
        let mut tokens = cancel_tokens.write().unwrap();
        tokens.remove(&request_id);
    }

    result
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn link_manual_file(
    file_path: String,
    tmdb_id: String,
    season_num: u32,
    ep_num: u32,
) -> Result<(), AppError> {
    handle_panic(|| {
        let mut conn = get_db_connection()?;
        let mut ep_id: Option<i32> = None;

        let media_id: i32 = match conn.query_row(
            "SELECT id FROM Media WHERE tmdb_id = ?",
            params![tmdb_id],
            |row| row.get(0),
        ) {
            Ok(id) => id,
            Err(_) => {
                return Err(AppError::Custom(format!(
                    "Show TMDB ID {} not found in library.",
                    tmdb_id
                )))
            }
        };

        if let Ok(id) = conn.query_row(
            "SELECT id FROM Episodes WHERE media_id = ? AND season_num = ? AND ep_num = ?",
            params![media_id, season_num, ep_num],
            |row| row.get(0),
        ) {
            ep_id = Some(id);
        }

        if let Some(id) = ep_id {
            let tx = conn.transaction()?;
            tx.execute("INSERT INTO Local_Files (episode_id, file_path) VALUES (?, ?) ON CONFLICT(episode_id) DO UPDATE SET file_path=excluded.file_path", params![id, file_path])?;
            tx.execute(
                "DELETE FROM Unmatched_Files WHERE file_path = ?",
                params![file_path],
            )?;
            tx.commit()?;
            Ok(())
        } else {
            Err(AppError::Custom(format!(
                "Episode S{:02}E{:02} not found in database for this show.",
                season_num, ep_num
            )))
        }
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub fn toggle_episode_status(
    episode_id: i32,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    handle_panic(std::panic::AssertUnwindSafe(|| {
        let conn = get_db_connection()?;

        // Fetch current status
        let mut current_status = "Unwatched".to_string();
        {
            let mut stmt = conn.prepare("SELECT status FROM Episodes WHERE id = ?")?;
            let mut rows = stmt.query(params![episode_id])?;
            if let Some(row) = rows.next()? {
                current_status = row
                    .get::<_, Option<String>>(0)?
                    .unwrap_or("Unwatched".to_string());
            }
        }

        if current_status == "Completed" {
            // Unwatch
            // Watch_count doesn't decrement for safety in case of rewatches, but last_position strictly resets
            conn.execute(
                "UPDATE Episodes SET status = 'Unwatched', last_position = 0 WHERE id = ?",
                params![episode_id],
            )?;
            // Remove recent history
            // Just delete the most recent completion for this episode
            conn.execute(
                "DELETE FROM History WHERE id = (SELECT id FROM History WHERE episode_id = ? ORDER BY timestamp DESC LIMIT 1)",
                params![episode_id]
            )?;
        } else {
            // Watch
            conn.execute(
                "UPDATE Episodes SET status = 'Completed', watch_count = watch_count + 1, last_position = 0 WHERE id = ?",
                params![episode_id]
            )?;
            let session_id = uuid::Uuid::new_v4().to_string();
            let current_timestamp = chrono::Utc::now().timestamp();
            conn.execute(
                "INSERT INTO History (episode_id, timestamp, is_legacy, session_id, status, completion_ratio) VALUES (?, ?, 0, ?, 'Completed', 1.0)",
                params![episode_id, current_timestamp, session_id]
            )?;
        }

        if let Ok(mut cache) = state.stats_cache.write() {
            *cache = None;
        }

        Ok(())
    }))
}

#[cfg(test)]
#[path = "commands_tests_export.rs"]
mod commands_tests_export;

#[cfg(test)]
#[path = "commands_tests_optimize.rs"]
mod commands_tests_optimize;

#[cfg(test)]
#[path = "commands_tests_tmdb_auth.rs"]
mod commands_tests_tmdb_auth;

#[cfg(test)]
#[path = "commands_tests_episodes.rs"]
mod commands_tests_episodes;

#[cfg(test)]
#[path = "commands_tests_feature_3_6.rs"]
mod commands_tests_feature_3_6;

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn check_path_exists(path: String) -> Result<Option<String>, AppError> {
    let path_buf = std::path::PathBuf::from(&path);

    // Resolve relative to absolute if needed
    let resolved = if path_buf.is_relative() {
        std::env::current_dir().unwrap_or_default().join(path_buf)
    } else {
        path_buf
    };

    if resolved.exists() {
        // Normalize the path
        if let Ok(canonical) = dunce::canonicalize(&resolved) {
            return Ok(Some(canonical.to_string_lossy().into_owned()));
        }
        return Ok(Some(resolved.to_string_lossy().into_owned()));
    }

    Ok(None)
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub fn auto_detect_vlc() -> Result<Option<String>, AppError> {
    let mut paths: Vec<std::path::PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(prog_files) = std::env::var("ProgramFiles") {
            paths.push(std::path::PathBuf::from(format!(
                r"{}\VideoLAN\VLC\vlc.exe",
                prog_files
            )));
        }
        if let Ok(prog_files_x86) = std::env::var("ProgramFiles(x86)") {
            paths.push(std::path::PathBuf::from(format!(
                r"{}\VideoLAN\VLC\vlc.exe",
                prog_files_x86
            )));
        }
        if let Ok(app_data) = std::env::var("LOCALAPPDATA") {
            paths.push(std::path::PathBuf::from(format!(
                r"{}\Programs\VLC\vlc.exe",
                app_data
            )));
        }
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(std::path::PathBuf::from(
            "/Applications/VLC.app/Contents/MacOS/VLC",
        ));
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(std::path::PathBuf::from("/usr/bin/vlc"));
        paths.push(std::path::PathBuf::from("/usr/local/bin/vlc"));
        paths.push(std::path::PathBuf::from(
            "/var/lib/flatpak/exports/bin/org.videolan.VLC",
        ));
        paths.push(std::path::PathBuf::from("/snap/bin/vlc"));
    }

    for path in paths {
        if path.exists() {
            // Check execute permissions
            if let Ok(_metadata) = std::fs::metadata(&path) {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let permissions = _metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        if let Ok(canonical) = dunce::canonicalize(&path) {
                            return Ok(Some(canonical.to_string_lossy().into_owned()));
                        }
                    }
                }

                #[cfg(windows)]
                {
                    // Simple check for Windows (can attempt to open with read_execute later if needed)
                    // Currently relying on standard execute checks by trusting the .exe extension and existence
                    if let Ok(canonical) = dunce::canonicalize(&path) {
                        return Ok(Some(canonical.to_string_lossy().into_owned()));
                    }
                }
            }
        }
    }

    Ok(None)
}

#[tauri::command]
pub fn frontend_log(level: String, message: String, context: Option<String>) {
    let msg = match context {
        Some(c) => format!("{} | Context: {}", message, c),
        None => message,
    };

    match level.to_lowercase().as_str() {
        "error" => tracing::error!("{}", msg),
        "warn" => tracing::warn!("{}", msg),
        "info" => tracing::info!("{}", msg),
        "debug" => tracing::debug!("{}", msg),
        _ => tracing::info!("{}", msg), // default to info
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn update_media_rating(
    state: tauri::State<'_, AppState>,
    media_id: i32,
    rating: Option<i32>,
) -> Result<(), AppError> {
    if let Some(r) = rating {
        if r < 0 || r > 10 {
            let error_msg = format!("Validation Error: rating {} is out of bounds (0-10)", r);
            tracing::error!("{}", error_msg);
            return Err(AppError::Custom(error_msg));
        }
    }

    let db_queue = state.db_queue.clone();
    db_queue.push_high_priority_action(crate::task_queue::DbAction::UpdateMediaRating(
        media_id, rating,
    ));

    if let Ok(mut cache) = state.stats_cache.write() {
        *cache = None;
    }

    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn get_upcoming_airings() -> Result<Vec<Value>, AppError> {
    crate::auto_updater::get_upcoming_airings_db()
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn get_new_season_alerts() -> Result<Vec<Value>, AppError> {
    crate::auto_updater::get_new_season_alerts_db()
}

#[tauri::command]
#[tracing::instrument(level = "debug")]
pub async fn dismiss_new_season_alert(media_id: i32) -> Result<(), AppError> {
    crate::auto_updater::dismiss_new_season_alert_db(media_id)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip(app))]
pub async fn trigger_auto_sync(app: tauri::AppHandle) -> Result<usize, AppError> {
    crate::auto_updater::run_background_auto_update(app, true).await
}

/// Checks TMDB reachability right away (the banner's Retry button, and the
/// frontend's initial state), publishes the result and resumes deferred work
/// if TMDB is reachable again.
#[tauri::command]
#[tracing::instrument(level = "debug", skip(app))]
pub async fn check_connectivity(
    app: tauri::AppHandle,
) -> Result<crate::network::ConnectivityReport, AppError> {
    let report = crate::network::check_connectivity().await;
    crate::auto_updater::publish_connectivity(&app, &report);
    Ok(report)
}

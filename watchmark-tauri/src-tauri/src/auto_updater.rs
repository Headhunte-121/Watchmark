// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw println! allowed.

use crate::db::{get_db_connection, get_readonly_connection};
use crate::error::AppError;
use crate::settings::{load_settings, save_settings};
use crate::tmdb::{get_media_details, get_tv_season_episodes, resolve_local_backdrop_path, resolve_local_poster_path};
use chrono::{Local, NaiveDate};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::network::{self, Connectivity, ConnectivityReport};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedShow {
    pub id: i32,
    pub tmdb_id: String,
    pub title: String,
    pub total_episodes: i32,
    pub max_season: u32,
    pub local_ep_count: i64,
}

pub fn get_tracked_shows_db() -> Result<Vec<TrackedShow>, AppError> {
    let conn = get_readonly_connection()?;
    let mut stmt = conn.prepare(
        "SELECT m.id, m.tmdb_id, m.title, COALESCE(m.total_episodes, 0),
                COALESCE(MAX(e.season_num), 0) as max_season,
                COUNT(e.id) as local_ep_count
         FROM Media m
         LEFT JOIN Episodes e ON m.id = e.media_id
         WHERE m.type = 'TV' AND (
             EXISTS (SELECT 1 FROM History h JOIN Episodes ep ON h.episode_id = ep.id WHERE ep.media_id = m.id)
             OR EXISTS (SELECT 1 FROM Local_Files lf JOIN Episodes ep ON lf.episode_id = ep.id WHERE ep.media_id = m.id)
         )
         GROUP BY m.id",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(TrackedShow {
            id: row.get(0)?,
            tmdb_id: row.get(1)?,
            title: row.get(2)?,
            total_episodes: row.get(3)?,
            max_season: row.get(4)?,
            local_ep_count: row.get(5)?,
        })
    })?;

    let mut shows = Vec::new();
    for row in rows {
        if let Ok(show) = row {
            shows.push(show);
        }
    }
    Ok(shows)
}

pub async fn sync_single_show(api_key: &str, show: &TrackedShow) -> Result<bool, AppError> {
    tracing::info!(action = "auto_update_probe", show = %show.title, tmdb_id = %show.tmdb_id, "Checking show updates on TMDB");

    let details = get_media_details(api_key, &show.tmdb_id, "TV").await?;

    let tmdb_total_seasons = details["number_of_seasons"].as_u64().unwrap_or(0) as u32;
    let tmdb_total_episodes = details["total_episodes"].as_i64().unwrap_or(0) as i32;

    let mut updated = false;
    let mut new_season_detected = false;
    let mut highest_new_season = 0;

    // Check if new seasons have been added to TMDB beyond what we have locally
    if tmdb_total_seasons > show.max_season {
        for season_num in (show.max_season + 1)..=tmdb_total_seasons {
            tracing::info!(action = "auto_update_new_season", show = %show.title, season = season_num, "New season detected on TMDB, fetching episodes");
            if let Ok(episodes) = get_tv_season_episodes(api_key, &show.tmdb_id, season_num).await {
                if !episodes.is_empty() {
                    let mut conn = get_db_connection()?;
                    let tx = conn.transaction()?;
                    for ep in episodes {
                        let season_number = ep["season_num"].as_u64().unwrap_or(season_num as u64) as u32;
                        let episode_number = ep["ep_num"].as_u64().unwrap_or(0) as u32;
                        let title = ep["title"].as_str().unwrap_or("Unknown Title");
                        let runtime = ep["runtime"].as_i64().unwrap_or(0) as i32;
                        let still_path = ep["still_path"].as_str().unwrap_or("");
                        let overview = ep["overview"].as_str().unwrap_or("");
                        let air_date = ep["air_date"].as_str().unwrap_or("");
                        let is_exact = ep["is_exact_date"].as_bool().unwrap_or(!air_date.is_empty());

                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, air_date, is_exact_date)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                            params![
                                show.id,
                                season_number,
                                episode_number,
                                title,
                                runtime,
                                still_path,
                                overview,
                                air_date,
                                is_exact
                            ],
                        );
                    }
                    let _ = tx.commit();
                    updated = true;
                    new_season_detected = true;
                    if season_num > highest_new_season {
                        highest_new_season = season_num;
                    }
                }
            }
        }
    }

    // Also check the latest active season for newly added/scheduled episodes or updated air dates
    let target_season = if tmdb_total_seasons > 0 {
        tmdb_total_seasons
    } else {
        show.max_season
    };

    if target_season > 0 {
        if let Ok(episodes) = get_tv_season_episodes(api_key, &show.tmdb_id, target_season).await {
            let mut conn = get_db_connection()?;
            let tx = conn.transaction()?;
            for ep in episodes {
                let season_number = ep["season_num"].as_u64().unwrap_or(target_season as u64) as u32;
                let episode_number = ep["ep_num"].as_u64().unwrap_or(0) as u32;
                let ep_title = ep["title"].as_str().unwrap_or("Unknown Title");
                let air_date = ep["air_date"].as_str().unwrap_or("");
                let still_path = ep["still_path"].as_str().unwrap_or("");
                let overview = ep["overview"].as_str().unwrap_or("");
                let runtime = ep["runtime"].as_i64().unwrap_or(0) as i32;
                let is_exact = ep["is_exact_date"].as_bool().unwrap_or(!air_date.is_empty());

                let res = tx.execute(
                    "INSERT INTO Episodes (media_id, season_num, ep_num, \"title\", runtime, still_path, overview, air_date, is_exact_date)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(media_id, season_num, ep_num) DO UPDATE SET
                         \"title\" = excluded.title,
                         air_date = CASE WHEN (air_date IS NULL OR air_date = '' OR air_date = '0000-00-00') THEN excluded.air_date ELSE air_date END,
                         still_path = CASE WHEN (still_path IS NULL OR still_path = '') THEN excluded.still_path ELSE still_path END,
                         runtime = CASE WHEN (runtime IS NULL OR runtime = 0) THEN excluded.runtime ELSE runtime END",
                    params![
                        show.id,
                        season_number,
                        episode_number,
                        ep_title,
                        runtime,
                        still_path,
                        overview,
                        air_date,
                        is_exact
                    ],
                );
                if res.is_ok() {
                    updated = true;
                }
            }
            let _ = tx.commit();
        }
    }

    // Update Media total_episodes if TMDB has more
    if tmdb_total_episodes > show.total_episodes {
        let conn = get_db_connection()?;
        let _ = conn.execute(
            "UPDATE Media SET total_episodes = ? WHERE id = ?",
            params![tmdb_total_episodes, show.id],
        );
        updated = true;
    }

    // If a brand new season was detected, register a Season_Alerts notification
    if new_season_detected && highest_new_season > 0 {
        let conn = get_db_connection()?;
        let _ = conn.execute(
            "INSERT INTO Season_Alerts (media_id, season_num, dismissed)
             VALUES (?, ?, 0)
             ON CONFLICT(media_id) DO UPDATE SET season_num = excluded.season_num, dismissed = 0",
            params![show.id, highest_new_season],
        );
    }

    Ok(updated)
}

pub fn get_upcoming_airings_db() -> Result<Vec<Value>, AppError> {
    let conn = get_readonly_connection()?;
    let mut stmt = conn.prepare(
        "SELECT e.id as episode_id, e.season_num, e.ep_num, e.title as ep_title, e.air_date, e.still_path,
                e.overview, m.id as media_id, m.title as show_title, m.poster_path, m.backdrop_path, m.networks
         FROM Episodes e
         JOIN Media m ON e.media_id = m.id
         WHERE m.type = 'TV'
           AND e.air_date IS NOT NULL
           AND e.air_date != ''
           AND e.air_date != '0000-00-00'
           AND e.air_date >= date('now', 'localtime')
           AND e.air_date <= date('now', 'localtime', '+7 days')
           AND (
               EXISTS (SELECT 1 FROM History h JOIN Episodes ep ON h.episode_id = ep.id WHERE ep.media_id = m.id)
               OR EXISTS (SELECT 1 FROM Local_Files lf JOIN Episodes ep ON lf.episode_id = ep.id WHERE ep.media_id = m.id)
           )
         ORDER BY e.air_date ASC, e.season_num ASC, e.ep_num ASC",
    )?;

    let today_str = Local::now().format("%Y-%m-%d").to_string();
    let today = NaiveDate::parse_from_str(&today_str, "%Y-%m-%d")
        .unwrap_or_else(|_| Local::now().date_naive());

    let mut airings = Vec::new();
    let mut rows = stmt.query([])?;

    while let Ok(Some(row)) = rows.next() {
        let air_date_str: String = row.get(4)?;
        let parsed_air_date = NaiveDate::parse_from_str(&air_date_str, "%Y-%m-%d");

        let (days_until, is_today, is_tomorrow, relative_str) = match parsed_air_date {
            Ok(d) => {
                let diff = (d - today).num_days();
                let weekday = d.format("%A").to_string();
                if diff == 0 {
                    (0, true, false, "Airing Today".to_string())
                } else if diff == 1 {
                    (1, false, true, "Tomorrow".to_string())
                } else {
                    (diff, false, false, format!("In {} days ({})", diff, weekday))
                }
            }
            Err(_) => (0, false, false, air_date_str.clone()),
        };

        let raw_poster: String = row.get::<_, Option<String>>(9)?.unwrap_or_default();
        let resolved_poster = resolve_local_poster_path(&raw_poster, "w500", false).unwrap_or(raw_poster);
        let raw_backdrop: String = row.get::<_, Option<String>>(10)?.unwrap_or_default();
        let resolved_backdrop = resolve_local_backdrop_path(&raw_backdrop, "w1280", false).unwrap_or(raw_backdrop);

        airings.push(json!({
            "episode_id": row.get::<_, i32>(0)?,
            "season_num": row.get::<_, u32>(1)?,
            "ep_num": row.get::<_, u32>(2)?,
            "ep_title": row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            "air_date": air_date_str,
            "still_path": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            "overview": row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            "media_id": row.get::<_, i32>(7)?,
            "show_title": row.get::<_, Option<String>>(8)?.unwrap_or_default(),
            "poster_path": resolved_poster,
            "backdrop_path": resolved_backdrop,
            "network": row.get::<_, Option<String>>(11)?.unwrap_or_default(),
            "days_until": days_until,
            "is_today": is_today,
            "is_tomorrow": is_tomorrow,
            "relative_air_string": relative_str,
        }));
    }

    Ok(airings)
}

pub fn get_new_season_alerts_db() -> Result<Vec<Value>, AppError> {
    let conn = get_readonly_connection()?;
    let mut stmt = conn.prepare(
        "SELECT sa.media_id, sa.season_num, m.title as show_title, m.poster_path, m.backdrop_path,
                COUNT(e.id) as episode_count
         FROM Season_Alerts sa
         JOIN Media m ON sa.media_id = m.id
         LEFT JOIN Episodes e ON m.id = e.media_id AND e.season_num = sa.season_num
         WHERE sa.dismissed = 0
           AND NOT EXISTS (
               SELECT 1 FROM History h
               JOIN Episodes ep ON h.episode_id = ep.id
               WHERE ep.media_id = sa.media_id AND ep.season_num = sa.season_num
           )
         GROUP BY sa.media_id, sa.season_num
         ORDER BY sa.created_at DESC",
    )?;

    let mut alerts = Vec::new();
    let mut rows = stmt.query([])?;

    while let Ok(Some(row)) = rows.next() {
        let raw_poster: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
        let resolved_poster = resolve_local_poster_path(&raw_poster, "w500", false).unwrap_or(raw_poster);
        let raw_backdrop: String = row.get::<_, Option<String>>(4)?.unwrap_or_default();
        let resolved_backdrop = resolve_local_backdrop_path(&raw_backdrop, "w1280", false).unwrap_or(raw_backdrop);

        alerts.push(json!({
            "media_id": row.get::<_, i32>(0)?,
            "season_num": row.get::<_, u32>(1)?,
            "show_title": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            "poster_path": resolved_poster,
            "backdrop_path": resolved_backdrop,
            "episode_count": row.get::<_, i64>(5)?,
        }));
    }

    Ok(alerts)
}

pub fn dismiss_new_season_alert_db(media_id: i32) -> Result<(), AppError> {
    let conn = get_db_connection()?;
    conn.execute(
        "UPDATE Season_Alerts SET dismissed = 1 WHERE media_id = ?",
        params![media_id],
    )?;
    Ok(())
}

pub async fn run_background_auto_update(app: AppHandle, force: bool) -> Result<usize, AppError> {
    let settings = match load_settings() {
        Ok(s) => s,
        Err(e) => return Err(AppError::Custom(e)),
    };

    if settings.tmdb_api_key.trim().is_empty() {
        return Ok(0);
    }

    let now = chrono::Utc::now().timestamp();
    // 12 hours = 43200 seconds
    if !force && (now - settings.last_auto_sync_timestamp) < 43200 {
        tracing::debug!(action = "auto_update_skip", "Auto-update skipped: ran within 12 hours");
        return Ok(0);
    }

    let tracked_shows = match get_tracked_shows_db() {
        Ok(s) => s,
        Err(e) => return Err(e),
    };

    if tracked_shows.is_empty() {
        return Ok(0);
    }

    // Don't start a sync TMDB can't answer. Mark it pending instead: it runs
    // as soon as connectivity is restored, and the "last synced" time is left
    // untouched so the 12-hour guard doesn't swallow the retry.
    let mut state = network::connectivity();
    if state == Connectivity::Unknown {
        state = network::check_connectivity().await.status;
    }
    if state.is_degraded() {
        AUTO_SYNC_PENDING.store(true, Ordering::SeqCst);
        tracing::info!(action = "auto_update_deferred", status = ?state, "TMDB unreachable; background sync deferred until it is reachable");
        return Ok(0);
    }

    if AUTO_SYNC_RUNNING.swap(true, Ordering::SeqCst) {
        tracing::debug!(action = "auto_update_skip", "Auto-update already running");
        return Ok(0);
    }
    struct RunningGuard;
    impl Drop for RunningGuard {
        fn drop(&mut self) {
            AUTO_SYNC_RUNNING.store(false, Ordering::SeqCst);
        }
    }
    let _running = RunningGuard;

    tracing::info!(action = "auto_update_start", count = tracked_shows.len(), "Starting background auto-update for tracked shows");

    let mut updated_count = 0;
    for show in tracked_shows {
        match sync_single_show(&settings.tmdb_api_key, &show).await {
            Ok(has_updates) => {
                if has_updates {
                    updated_count += 1;
                }
            }
            Err(e) if is_connectivity_error(&e) => {
                // Every remaining show would fail the same way: stop, keep the
                // previous sync time, and resume when TMDB is reachable again.
                AUTO_SYNC_PENDING.store(true, Ordering::SeqCst);
                tracing::warn!(action = "auto_update_aborted", show = %show.title, error = ?e, "TMDB became unreachable; background sync paused until it is reachable");
                if updated_count > 0 {
                    let _ = app.emit("shows-auto-updated", json!({ "updated_count": updated_count }));
                }
                return Ok(updated_count);
            }
            Err(e) => {
                tracing::warn!(action = "auto_update_error", show = %show.title, error = ?e, "Failed to auto-update show");
            }
        }
        // Spacing requests by 250ms to strictly comply with TMDB rate limits
        sleep(Duration::from_millis(250)).await;
    }

    // Save updated timestamp
    let mut updated_settings = settings;
    updated_settings.last_auto_sync_timestamp = now;
    let _ = save_settings(&updated_settings);

    if updated_count > 0 {
        tracing::info!(action = "auto_update_complete", updated = updated_count, "Background auto-update completed with updates");
        let _ = app.emit("shows-auto-updated", json!({ "updated_count": updated_count }));
    }

    Ok(updated_count)
}

// ---------------------------------------------------------------------------
// Connectivity: publishing status and recovering after an outage
// ---------------------------------------------------------------------------

/// Set when a background sync was skipped or interrupted because TMDB was
/// unreachable; cleared when the deferred sync starts.
pub static AUTO_SYNC_PENDING: AtomicBool = AtomicBool::new(false);
static AUTO_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);
/// Last status sent to the UI, so transitions are detected even when the
/// status was changed by something other than the heartbeat.
static LAST_PUBLISHED: AtomicU8 = AtomicU8::new(Connectivity::Unknown as u8);

fn is_connectivity_error(e: &AppError) -> bool {
    matches!(
        e,
        AppError::TmdbUnreachable | AppError::NetworkOffline | AppError::NetworkTimeout
    )
}

/// Sends a connectivity report to the UI and, when TMDB is reachable, starts
/// any work that was deferred while it wasn't.
pub fn publish_connectivity(app: &AppHandle, report: &ConnectivityReport) {
    let previous = LAST_PUBLISHED.swap(report.status as u8, Ordering::SeqCst);
    let _ = app.emit("network-status", report);

    if report.status != Connectivity::Online {
        return;
    }

    let recovered = previous == Connectivity::TmdbBlocked as u8 || previous == Connectivity::Offline as u8;
    if recovered {
        tracing::info!(action = "connectivity_restored", "[NETWORK] ✅ TMDB reachable again; resuming deferred work");
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            retry_failed_image_syncs(&handle).await;
        });
    }

    if AUTO_SYNC_PENDING.swap(false, Ordering::SeqCst) {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = run_background_auto_update(handle, false).await;
        });
    }
}

/// Retries image downloads that failed earlier. Images that fail again are
/// put back in the queue for the next attempt.
pub async fn retry_failed_image_syncs(app: &AppHandle) {
    if network::connectivity().is_degraded() {
        return;
    }
    let Some(state) = app.try_state::<crate::commands::AppState>() else {
        return;
    };
    let pending: Vec<(String, String)> = match state.failed_image_syncs.write() {
        Ok(mut queue) => queue.drain().collect(),
        Err(_) => return,
    };
    if pending.is_empty() {
        return;
    }
    let hp_mode = state.settings.read().map(|s| s.high_performance_mode).unwrap_or(false);
    let mut still_failing = Vec::new();
    for (path, size) in pending {
        tokio::task::yield_now().await;
        if crate::tmdb::download_image(&path, &size, hp_mode).await.is_none() {
            still_failing.push((path, size));
        }
    }
    if !still_failing.is_empty() {
        if let Ok(mut queue) = state.failed_image_syncs.write() {
            queue.extend(still_failing);
        }
    }
}

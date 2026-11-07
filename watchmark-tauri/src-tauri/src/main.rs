// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw println! allowed.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
pub mod auto_updater;
mod backup;
mod commands;
mod db;
mod error;
mod filesystem_guard;
mod hash;
mod logging;
mod models;
pub mod network;
mod sanitizer;
mod scanner;
mod settings;
pub mod task_queue;
mod tmdb;
mod vlc;

#[cfg(test)]
#[path = "sanitizer_tests.rs"]
mod sanitizer_tests;

#[cfg(test)]
#[path = "backup_tests.rs"]
mod backup_tests;

#[cfg(test)]
#[path = "restore_tests.rs"]
mod restore_tests;

#[cfg(test)]
#[path = "task_queue_tests.rs"]
mod task_queue_tests;

#[cfg(test)]
fn backup_tests_module_trigger() {}

use native_dialog::{MessageDialog, MessageType};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_notification::NotificationExt;

pub fn execute_cold_swap(app_dir: &std::path::Path) {
    let trigger_file_path = app_dir.join(".restore_pending");

    if trigger_file_path.exists() {
        let db_dir = app_dir.join("db");
        let active_db_path = db_dir.join("watchmark.db");
        let active_wal_path = db_dir.join("watchmark.db-wal");
        let active_shm_path = db_dir.join("watchmark.db-shm");

        let pending_db_path = db_dir.join("watchmark.db.pending");
        let pending_sha256_path = db_dir.join("watchmark.db.pending.sha256");

        let old_db_path = db_dir.join("watchmark.db.old");
        let old_wal_path = db_dir.join("watchmark.db.old-wal");
        let old_shm_path = db_dir.join("watchmark.db.old-shm");

        // Only proceed if the pending database was successfully staged
        if pending_db_path.exists() {
            // Verify pending DB integrity if SHA-256 sidecar is present
            if pending_sha256_path.exists() {
                if let Err(e) = crate::backup::verify_sha256_sidecar(&pending_db_path) {
                    tracing::error!("Corrupted pending database detected during cold swap: {}", e);
                    let _ = std::fs::remove_file(&pending_db_path);
                    let _ = std::fs::remove_file(&pending_sha256_path);
                    let _ = std::fs::remove_file(&trigger_file_path);
                    return;
                }
            }

            // Move active to .old (overwrite if exists)
            if active_db_path.exists() {
                let _ = std::fs::rename(&active_db_path, &old_db_path);
            }
            // Rotate stale WAL/SHM files to prevent replay into the newly restored DB
            if active_wal_path.exists() {
                let _ = std::fs::rename(&active_wal_path, &old_wal_path);
            }
            if active_shm_path.exists() {
                let _ = std::fs::rename(&active_shm_path, &old_shm_path);
            }

            // Move pending to active
            if let Err(e) = std::fs::rename(&pending_db_path, &active_db_path) {
                // Critical failure during rename. Attempt to revert.
                if old_db_path.exists() {
                    let _ = std::fs::rename(&old_db_path, &active_db_path);
                }
                if old_wal_path.exists() {
                    let _ = std::fs::rename(&old_wal_path, &active_wal_path);
                }
                if old_shm_path.exists() {
                    let _ = std::fs::rename(&old_shm_path, &active_shm_path);
                }

                let error_msg = format!("Fatal Error: Database Restore Failed during cold-swap. Changes reverted.\n\nError details: {}", e);
                MessageDialog::new()
                    .set_type(MessageType::Error)
                    .set_title("WatchMark - Restore Error")
                    .set_text(&error_msg)
                    .show_alert()
                    .unwrap();
            } else {
                // Success! Clean up the trigger file and pending sidecar.
                let _ = std::fs::remove_file(&trigger_file_path);
                if pending_sha256_path.exists() {
                    let _ = std::fs::remove_file(&pending_sha256_path);
                }
            }
        } else {
            // Trigger file exists but pending DB is missing. Corrupt state. Clean up flag.
            let _ = std::fs::remove_file(&trigger_file_path);
            if pending_sha256_path.exists() {
                let _ = std::fs::remove_file(&pending_sha256_path);
            }
        }
    }
}

fn main() {
    let app_dir = db::get_app_data_dir();

    // Load settings early to get logging config, but fall back gracefully if we can't write to disk yet.
    let initial_settings = match settings::load_settings() {
        Ok(s) => s,
        Err(_e) => {
            // If settings fail to load initially (e.g. disk issue), we create a default config
            // for the logger and let the File System Guard catch the actual disk problem in `.setup()`.
            models::Settings::default()
        }
    };

    // Initialize Tracing Engine First
    logging::init_tracing(
        &initial_settings.global_log_level,
        &initial_settings.module_logs,
    );

    tracing::info!(
        action = "boot",
        "[APP] 🚀 Starting application WatchMark..."
    );

    // Cold-Swap Database Restore Logic
    execute_cold_swap(&app_dir);

    let (settings_tx, mut settings_rx) = tokio::sync::mpsc::channel::<models::Settings>(100);

    // Spawn Background Backup Task
    let backup_settings_arc = std::sync::Arc::new(std::sync::RwLock::new(initial_settings.clone()));
    let app_backup_settings_arc = backup_settings_arc.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app_handle = window.app_handle().clone();

                // Save window bounds before exiting
                if let Ok(outer_pos) = window.outer_position() {
                    if let Ok(outer_size) = window.inner_size() {
                        if let Some(state) = app_handle.try_state::<commands::AppState>() {
                            if let Ok(mut settings) = state.settings.write() {
                                let is_maximized = window.is_maximized().unwrap_or(false);
                                let is_fullscreen = window.is_fullscreen().unwrap_or(false);

                                settings.x = outer_pos.x;
                                settings.y = outer_pos.y;
                                settings.is_maximized = is_maximized;
                                settings.is_fullscreen = is_fullscreen;

                                if !is_maximized && !is_fullscreen {
                                    let scale_factor = window.scale_factor().unwrap_or(1.0);
                                    settings.width = (outer_size.width as f64 / scale_factor) as i32;
                                    settings.height = (outer_size.height as f64 / scale_factor) as i32;
                                }

                                let _ = crate::settings::save_settings(&*settings);
                            }
                        }
                    }
                }


                if let Some(state) = app_handle.try_state::<commands::AppState>() {
                    let db_queue = state.db_queue.clone();

                    api.prevent_close(); // Prevent immediate exit
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    db_queue.shutdown(tx);

                    tauri::async_runtime::spawn(async move {
                        // Wait for maximum 2 seconds for worker thread to drain
                        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await;
                        app_handle.exit(0);
                    });
                }
            }
        })
        .setup(move |app| {
            tracing::info!("[APP] 🚀 Tauri setup hook triggered. Initializing state...");

            // 0. Restore Window Geometry
            let window = app.get_webview_window("main").unwrap();
            let mut x = initial_settings.x;
            let mut y = initial_settings.y;
            let width = initial_settings.width;
            let height = initial_settings.height;

            if x != -1 && y != -1 {
                let mut on_screen = false;
                if let Ok(monitors) = window.available_monitors() {
                    for monitor in monitors {
                        let position = monitor.position();
                        let size = monitor.size();

                        let is_x_visible = x >= position.x && x < (position.x + size.width as i32);
                        let is_y_visible = y >= position.y && y < (position.y + size.height as i32);
                        if is_x_visible && is_y_visible {
                            on_screen = true;
                            break;
                        }
                    }
                }

                if !on_screen {
                    // Fallback to center on primary
                    if let Ok(Some(monitor)) = window.primary_monitor() {
                        let position = monitor.position();
                        let size = monitor.size();
                        x = position.x + (size.width as i32 - width) / 2;
                        y = position.y + (size.height as i32 - height) / 2;
                    }
                }

                let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
            }

            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width: width as f64, height: height as f64 }));

            if initial_settings.is_maximized {
                let _ = window.maximize();
            }

            if initial_settings.is_fullscreen {
                let _ = window.set_fullscreen(true);
            }


            // 1. Boot-Time Filesystem Guard
            let app_data_dir = db::get_app_data_dir();
            filesystem_guard::execute_guard_and_exit_on_failure(app, &app_data_dir);

            // 2. Initialize Database after filesystem is vouched for
            tracing::info!(action = "init_db", "[DB] 🗄️ Initializing SQLite database...");
            if let Err(e) = db::init_db() {
                // Critical DB failure
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                app.dialog()
                    .message(format!("Fatal Error: WatchMark could not initialize its database.\nError: {}", e))
                    .kind(MessageDialogKind::Error)
                    .title("WatchMark - Fatal DB Error")
                    .blocking_show();
                std::process::exit(1);
            }
            tracing::info!(action = "init_db_success", "[DB] ✨ Database initialized successfully.");

            // 2.1 Dynamically grant asset protocol permissions to the app data and cache directory
            let app_data_dir = db::get_app_data_dir();
            if let Err(e) = app.asset_protocol_scope().allow_directory(&app_data_dir, true) {
                tracing::warn!("[ASSET_PROTOCOL] Failed to allow app_data_dir: {}", e);
            } else {
                tracing::info!(action = "asset_scope_granted", path = ?app_data_dir, "[ASSET_PROTOCOL] Granted asset protocol scope for app data directory.");
            }

            #[cfg(windows)]
            {
                if let Ok(user_profile) = std::env::var("USERPROFILE") {
                    let _ = app.asset_protocol_scope().allow_directory(user_profile, true);
                }
            }

            if let Ok(settings) = settings::load_settings() {
                if !settings.last_scanned_path.is_empty() {
                    let _ = app.asset_protocol_scope().allow_directory(&settings.last_scanned_path, true);
                }
            }

            // 3. Ensure offline fonts are present
            tauri::async_runtime::spawn(async move {
                if let Err(e) = assets::ensure_fonts().await {
                    tracing::error!(action = "init_fonts_error", "[ASSETS] ❌ Failed to ensure fonts: {}", e);
                }
            });

            let db_queue = std::sync::Arc::new(task_queue::DbTaskQueue::new(app.handle().clone()));

            let available_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
            let pool_size = (available_cores.saturating_sub(1)).max(1);

            let failed_image_sync_queue = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashSet::new()));

            app.manage(commands::AppState {
                settings: app_backup_settings_arc,
                settings_tx,
                db_queue: db_queue.clone(),
                is_maintenance_mode: std::sync::atomic::AtomicBool::new(false),
                is_api_authorized: std::sync::atomic::AtomicBool::new(true),
                is_rate_limited: std::sync::atomic::AtomicBool::new(false),
                rate_limit_reset: std::sync::atomic::AtomicI64::new(0),
                stats_cache: std::sync::Arc::new(std::sync::RwLock::new(None)),
                read_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(pool_size)),
                cancel_tokens: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
                failed_image_syncs: failed_image_sync_queue.clone(),
                is_scan_cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                is_scan_paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        live_playback_time: std::sync::Arc::new(std::sync::RwLock::new(0.0)),
            });
            tracing::info!("[APP] ⚙️ AppState successfully managed by Tauri. Thread pool restricted to {}", pool_size);

            #[cfg(test)]
            crate::backup_tests_module_trigger();

            let app_handle_for_backup = app.handle().clone();
            let app_handle_for_auto_update = app.handle().clone();

            // Background Auto-Updater Task (checks tracked shows for new seasons/episodes)
            tauri::async_runtime::spawn(async move {
                // Delay 5s to allow window and initial DB load to finish without resource contention
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let _ = auto_updater::run_background_auto_update(app_handle_for_auto_update, false).await;
            });

            // Background Backup Task
            tauri::async_runtime::spawn(async move {
                loop {
                    let mut should_backup = false;
                    let current_ts = chrono::Utc::now().timestamp();
                    {
                        if let Ok(settings) = backup_settings_arc.read() {
                            // 24 hours = 86400 seconds
                            if current_ts - settings.last_backup_timestamp > 86400 || settings.last_backup_timestamp == 0 {
                                should_backup = true;
                            }
                        }
                    }

                    if should_backup {
                        // Offload blocking backup task to a dedicated thread with low priority feel
                        let _app_h = app_handle_for_backup.clone();
                        let backup_res = tokio::task::spawn_blocking(move || {
                            crate::backup::perform_backup()
                        }).await;

                        match backup_res {
                            Ok(Ok(_)) => {
                                tracing::info!("[BACKUP] ✅ Automatic database backup successful.");
                                let mut next_settings = None;
                                {
                                    if let Ok(mut settings) = backup_settings_arc.write() {
                                        settings.last_backup_timestamp = current_ts;
                                        settings.last_backup_status = "success".to_string();
                                        settings.last_backup_error = "".to_string();
                                        next_settings = Some(settings.clone());
                                    }
                                }
                                if let Some(updated_settings) = next_settings {
                                    let _ = crate::settings::save_settings(&updated_settings);
                                    let _ = app_handle_for_backup.emit("backup-finished", serde_json::json!({
                                        "status": "success",
                                        "timestamp": current_ts
                                    }));
                                }
                            }
                            Ok(Err(e)) => {
                                tracing::error!("[BACKUP] 🚨 Automatic database backup failed: {}", e);
                                let fail_ts = chrono::Utc::now().timestamp();
                                let mut next_settings = None;
                                {
                                    if let Ok(mut settings) = backup_settings_arc.write() {
                                        settings.last_backup_status = "error".to_string();
                                        settings.last_backup_error = e.to_string();
                                        next_settings = Some(settings.clone());
                                    }
                                }
                                if let Some(updated_settings) = next_settings {
                                    let _ = crate::settings::save_settings(&updated_settings);
                                }
                                let _ = app_handle_for_backup.emit("backup-finished", serde_json::json!({
                                    "status": "error",
                                    "error": e.to_string(),
                                    "timestamp": fail_ts
                                }));
                            }
                            Err(_) => {
                                tracing::error!("Backup task panicked or timed out.");
                            }
                        }
                    }

                    // Sleep for 1 hour before checking again
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            });

            // Spawn low priority background image retry loop (runs every 30 mins)
            let app_handle_for_retry = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1800)); // 30 minutes
                loop {
                    interval.tick().await;

                    let mut retries = Vec::new();
                    {
                        let mut queue = failed_image_sync_queue.write().unwrap();
                        for item in queue.drain() {
                            retries.push(item);
                        }
                    }

                    if !retries.is_empty() {
                        if let Some(state) = app_handle_for_retry.try_state::<commands::AppState>() {
                            let hp_mode = state.settings.read().unwrap().high_performance_mode;
                            for (path, size) in retries {
                                tokio::task::yield_now().await; // prevent blocking
                                let _ = crate::tmdb::download_image(&path, &size, hp_mode).await;
                            }
                        }
                    }
                }
            });

            // Spawn 30-second heartbeat to detect offline mode
            let app_handle_for_heartbeat = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap_or_default();
                loop {
                    interval.tick().await;
                    // Attempt to ping 1.1.1.1 or TMDB base URL to ensure actual internet access
                    let is_online = match client.get("https://1.1.1.1").send().await {
                        Ok(res) => res.status().is_success(),
                        Err(_) => false,
                    };
                    let _ = app_handle_for_heartbeat.emit("network-status", serde_json::json!({ "online": is_online }));
                }
            });

            // Spawn debouncer task for saving settings inside Tauri's managed tokio runtime
            tauri::async_runtime::spawn(async move {
                let mut last_settings: Option<models::Settings> = None;
                let mut timeout = tokio::time::interval(tokio::time::Duration::from_millis(500));
                timeout.tick().await; // consume first tick immediately

                loop {
                    tokio::select! {
                        Some(settings) = settings_rx.recv() => {
                            last_settings = Some(settings);
                            timeout.reset();
                        }
                        _ = timeout.tick() => {
                            if let Some(settings) = last_settings.take() {
                                if let Err(e) = crate::settings::save_settings(&settings) {
                                    tracing::error!("Failed to save debounced settings: {}", e);
                                } else {
                                    tracing::info!("[APP] ⚙️ Settings successfully saved to disk.");
                                }
                            }
                        }
                    }
                }
            });
            use tauri_plugin_global_shortcut::{ShortcutState, GlobalShortcutExt};

            let _app_handle = app.handle().clone();

            // Register hotkey: Ctrl+Shift+S (Scan Directory)
            let scan_shortcut = "CommandOrControl+Shift+S";
            let hide_shortcut = "CommandOrControl+Shift+H";

            if let Err(e) = app.global_shortcut().on_shortcut(scan_shortcut, {
                move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("tray-scan", ());
                        }
                    }
                }
            }) {
                let _ = app.notification().builder().title("WatchMark").body(format!("Could not register hotkey {}: {:?}", scan_shortcut, e)).show();
            }

            if let Err(e) = app.global_shortcut().on_shortcut(hide_shortcut, {
                move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                }
            }) {
                let _ = app.notification().builder().title("WatchMark").body(format!("Could not register hotkey {}: {:?}", hide_shortcut, e)).show();
            }

            use tauri::menu::PredefinedMenuItem;
            let scan_i = MenuItem::with_id(app, "scan", "Scan Directory", true, None::<&str>)?;
            let resume_i = MenuItem::with_id(app, "resume", "Resume Last Show", true, None::<&str>)?;
            let update_i = MenuItem::with_id(app, "update", "Check for Updates", true, None::<&str>)?;
            let sep_i = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&scan_i, &resume_i, &update_i, &sep_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "scan" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.emit("tray-scan", ());
                            }
                        }
                        "resume" => {
                            // Resume last show logic to spawn VLC directly
                            if let Ok(conn) = db::get_db_connection() {
                                // Simplified: Query to get the last watched episode path.
                                // Assuming history table has timestamp and episode_id.
                                let result: rusqlite::Result<(i32, String)> = conn.query_row(
                                    "SELECT History.episode_id, Local_Files.file_path FROM History
                                     JOIN Local_Files ON History.episode_id = Local_Files.episode_id
                                     ORDER BY History.timestamp DESC LIMIT 1",
                                    [],
                                    |row: &rusqlite::Row| Ok((row.get(0)?, row.get(1)?)),
                                );

                                match result {
                                    Ok((ep_id, file_path)) => {
                                        // Fetch the position from episodes if available (optional)
                                        let last_position: i32 = conn.query_row("SELECT last_position FROM Episodes WHERE id = ?1", [ep_id], |row: &rusqlite::Row| row.get(0)).unwrap_or(0);

                                        // Spawn VLC (non-blocking) using vlc.rs functionality or directly.
                                        let app_handle = app.clone();

                                        tauri::async_runtime::spawn(async move {
                                            let app_handle_clone = app_handle.clone();

                                            // Handle potential panics from VLC or DB
                                            let play_result = tokio::task::spawn(async move {
                                                let state = app_handle_clone.state::<commands::AppState>();
                                                crate::vlc::play_episode_cmd(app_handle_clone.clone(), ep_id, file_path, serde_json::Value::from(last_position), state).await
                                            }).await;

                                            match play_result {
                                                Ok(Err(e)) => {
                                                    tracing::error!("Error playing from system tray: {}", e);
                                                }
                                                Err(e) => {
                                                    tracing::error!("Task panicked or cancelled from system tray: {:?}", e);
                                                }
                                                _ => {}
                                            }
                                        });
                                    }
                                    Err(_) => {
                                        let _ = app.notification()
                                            .builder()
                                            .title("WatchMark")
                                            .body("Cannot resume: File not found.")
                                            .show();
                                    }
                                }
                            }
                        }
                        "update" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.emit("check-for-updates", ());
                            }
                        }
                        "quit" => {
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| match event {
                    TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("watchmark", |_app, request, responder| {
            let path = request.uri().path();
            if path.is_empty() {
                responder.respond(
                    http::Response::builder()
                        .status(400)
                        .body(Vec::new())
                        .unwrap(),
                );
                return;
            }
            let path = path[1..].to_string();
            // URL decode the path
            let decoded_path = percent_encoding::percent_decode_str(&path)
                .decode_utf8_lossy()
                .to_string();

            let safe_path = std::path::PathBuf::from(&decoded_path);

            // Clone request headers before moving them into the task
            let range_header = request.headers().get("Range").and_then(|h| h.to_str().ok().map(|s| s.to_string()));

            tokio::spawn(async move {
                if !decoded_path.contains("..") && safe_path.is_absolute() {
                    if let Ok(real_path) = std::fs::canonicalize(&safe_path) {
                        if real_path.is_file() {
                            let mime_type = match real_path.extension().and_then(|e| e.to_str()) {
                                Some("png") => "image/png",
                                Some("jpg") | Some("jpeg") => "image/jpeg",
                                Some("webp") => "image/webp",
                                Some("mp4") => "video/mp4",
                                Some("mkv") => "video/x-matroska",
                                _ => "application/octet-stream",
                            };

                            if let Ok(metadata) = tokio::fs::metadata(&real_path).await {
                                let file_size = metadata.len();
                                use std::io::SeekFrom;
                                use tokio::io::AsyncSeekExt;
                                use tokio::io::AsyncReadExt;

                                if let Some(range_str) = range_header {
                                    // Parse Range header (e.g., "bytes=0-1023")
                                    let range_str = range_str.trim_start_matches("bytes=");
                                    let mut parts = range_str.split('-');
                                    let start_str = parts.next().unwrap_or("");
                                    let end_str = parts.next().unwrap_or("");

                                    let start = if start_str.is_empty() { 0 } else { start_str.parse::<u64>().unwrap_or(0) };

                                    // Default read chunk size is 1MB to keep memory footprint low
                                    let max_chunk_size = 1024 * 1024;
                                    let mut end = if end_str.is_empty() {
                                        std::cmp::min(start + max_chunk_size - 1, file_size - 1)
                                    } else {
                                        end_str.parse::<u64>().unwrap_or(file_size - 1)
                                    };

                                    if start >= file_size {
                                        responder.respond(
                                            http::Response::builder()
                                                .status(416) // Range Not Satisfiable
                                                .header("Content-Range", format!("bytes */{}", file_size))
                                                .body(Vec::new())
                                                .unwrap(),
                                        );
                                        return;
                                    }

                                    // Enforce max chunk size to prevent memory exhaustion on large requested ranges
                                    if end - start + 1 > max_chunk_size {
                                        end = start + max_chunk_size - 1;
                                    }

                                    // Ensure end doesn't exceed file boundaries
                                    end = std::cmp::min(end, file_size - 1);
                                    let chunk_size = end - start + 1;

                                    if let Ok(mut file) = tokio::fs::File::open(&real_path).await {
                                        if file.seek(SeekFrom::Start(start)).await.is_ok() {
                                            let mut buffer = vec![0; chunk_size as usize];
                                            if let Ok(bytes_read) = file.read_exact(&mut buffer).await {
                                                // Shrink buffer if we read less than expected (shouldn't happen with read_exact on local file but safe)
                                                buffer.truncate(bytes_read);

                                                responder.respond(
                                                    http::Response::builder()
                                                        .status(206) // Partial Content
                                                        .header("Access-Control-Allow-Origin", "*")
                                                        .header("Content-Type", mime_type)
                                                        .header("Accept-Ranges", "bytes")
                                                        .header("Content-Range", format!("bytes {}-{}/{}", start, start + bytes_read as u64 - 1, file_size))
                                                        .header("Content-Length", bytes_read.to_string())
                                                        .body(buffer)
                                                        .unwrap(),
                                                );
                                                return;
                                            }
                                        }
                                    }
                                } else {
                                    // No Range request: Return the file. For 2GB video files this is still bad,
                                    // but typical for images. However, to keep memory low, we can limit even 200 OK
                                    // responses to 10MB chunks if needed. But for standard compatibility, if the client
                                    // doesn't request a range, we should stream it. `responder.respond` accepts `Vec<u8>`.
                                    // If we read the whole file here, we still blow RAM. Let's just limit max 200 OK read to 30MB.
                                    // BUT, we can just send the first chunk and add `Accept-Ranges: bytes`.
                                    // For images, they are small. For videos, HTML5 ALWAYS sends Range.

                                    let limit = std::cmp::min(file_size, 1024 * 1024 * 30); // Max 30MB

                                    if let Ok(mut file) = tokio::fs::File::open(&real_path).await {
                                        let mut buffer = vec![0; limit as usize];
                                        if let Ok(bytes_read) = file.read(&mut buffer).await {
                                            buffer.truncate(bytes_read);
                                            responder.respond(
                                                http::Response::builder()
                                                    .status(200)
                                                    .header("Access-Control-Allow-Origin", "*")
                                                    .header("Content-Type", mime_type)
                                                    .header("Accept-Ranges", "bytes")
                                                    .header("Content-Length", file_size.to_string())
                                                    .body(buffer)
                                                    .unwrap(),
                                            );
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                responder.respond(
                    http::Response::builder()
                        .status(404)
                        .body(Vec::new())
                        .unwrap(),
                );
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::cancel_task,
            commands::optimize_database,
            commands::repair_paths,
            commands::remove_local_link,
            commands::validate_and_hash_file,
            commands::update_local_file,
            commands::get_live_playback_time,
            commands::get_settings,
            commands::save_settings,
            commands::update_log_settings,
            commands::get_available_modules,
            commands::delete_media_cmd,
            commands::get_media_details_db,
            commands::add_to_tracker,
            commands::mark_season_watched,
            commands::archive_season,
            commands::toggle_episode_status,
            commands::get_dashboard_data,
            commands::get_library_data,
            commands::get_next_episode_to_play,
            commands::play_next_episode_cmd,
            commands::clear_unmatched_files,
            commands::ignore_unmatched_group,
            commands::fetch_unmatched_files,
            commands::fetch_history,
            commands::delete_history_entries,
            commands::clear_history,
            commands::run_scan_directory,
            commands::cancel_active_scan,
            commands::pause_active_scan,
            commands::resume_active_scan,
            commands::perform_tmdb_search,
            commands::validate_tmdb_key,
            commands::assign_unmatched_to_tracker,
            commands::get_media_history_count,
            commands::get_app_data_dir,
            commands::update_media_rating,
            commands::export_database,
            commands::prepare_restore,
            commands::frontend_log,
            commands::check_path_exists,
            commands::auto_detect_vlc,
            vlc::play_episode_cmd,
            commands::link_manual_file,
            commands::get_upcoming_airings,
            commands::get_new_season_alerts,
            commands::dismiss_new_season_alert,
            commands::trigger_auto_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

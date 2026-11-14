// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw println! allowed.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DbError(#[from] rusqlite::Error),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("A critical backend error occurred: {0}")]
    Panic(String),

    #[error("Fatal Database/Permission error: {0}")]
    Fatal(String),

    #[error("Access Denied: {code} - {path}")]
    AccessDenied { code: String, path: String },

    #[error("{0}")]
    Custom(String),

    #[error("Network Timeout")]
    NetworkTimeout,

    #[error("Network Blocked (Firewall/Forbidden)")]
    NetworkBlocked,

    #[error("Network Offline")]
    NetworkOffline,

    /// The internet works but TMDB can't be reached (e.g. blocked by the ISP).
    #[error("TMDB is unreachable from this network")]
    TmdbUnreachable,
}

// Serialize the error to send to the frontend
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        match self {
            AppError::NetworkTimeout => {
                let mut state = serializer.serialize_struct("AppError", 3)?;
                state.serialize_field("type", "NetworkError")?;
                state.serialize_field("code", "TIMEOUT")?;
                state.serialize_field("message", "The connection timed out.")?;
                state.end()
            }
            AppError::NetworkBlocked => {
                let mut state = serializer.serialize_struct("AppError", 3)?;
                state.serialize_field("type", "NetworkError")?;
                state.serialize_field("code", "BLOCKED")?;
                state.serialize_field(
                    "message",
                    "Network Access Blocked. Please check your Windows Firewall settings.",
                )?;
                state.end()
            }
            AppError::NetworkOffline => {
                let mut state = serializer.serialize_struct("AppError", 3)?;
                state.serialize_field("type", "NetworkError")?;
                state.serialize_field("code", "OFFLINE")?;
                state.serialize_field("message", "You are currently offline.")?;
                state.end()
            }
            AppError::TmdbUnreachable => {
                let mut state = serializer.serialize_struct("AppError", 3)?;
                state.serialize_field("type", "NetworkError")?;
                state.serialize_field("code", "TMDB_BLOCKED")?;
                state.serialize_field(
                    "message",
                    "TMDB can't be reached from this network. Some internet providers block it; turning on Cloudflare WARP or using 1.1.1.1 as your DNS usually fixes this.",
                )?;
                state.end()
            }
            AppError::AccessDenied { code, path } => {
                let mut state = serializer.serialize_struct("AppError", 3)?;
                state.serialize_field("type", "AccessDenied")?;
                state.serialize_field("code", code)?;
                state.serialize_field("path", path)?;
                state.end()
            }
            _ => {
                let message = match self {
                    AppError::DbError(e) => format!("Database error: {}", e),
                    AppError::NetworkError(e) => format!("Network error: {}", e),
                    AppError::IoError(e) => format!("IO error: {}", e),
                    AppError::JsonError(e) => format!("Data Mismatch/Parse Error: {}", e),
                    AppError::Panic(ref msg) => format!("Critical Panic: {}", msg),
                    AppError::Fatal(ref msg) => format!("Fatal Error: {}", msg),
                    AppError::Custom(ref msg) => msg.to_string(),
                    _ => unreachable!(),
                };

                // For backward compatibility with existing errors that are just strings,
                // we can return an object or just string. Based on user's instruction,
                // we're modifying the new ones to be objects, and the old ones could
                // either stay as strings or be wrapped in an object.
                // Since Tauri automatically serializes `Result::Err` as the JSON payload,
                // React normally expects a string or object.
                // Let's return the string for backwards compatibility.
                serializer.serialize_str(&message)
            }
        }
    }
}

// Helper for generic From conversions if necessary (e.g. standard errors into Custom)
impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Custom(s.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Custom(s)
    }
}

pub fn handle_panic<F, R>(f: F) -> Result<R, AppError>
where
    F: FnOnce() -> Result<R, AppError> + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(f).unwrap_or_else(|err| {
        let msg = if let Some(s) = err.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = err.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "Unknown panic".to_string()
        };

        // Log the panic to the primary WatchMark.log in AppData
        let app_log_path = crate::db::get_app_data_dir().join("WatchMark.log");
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&app_log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] PANIC: {}", timestamp, msg);
        }

        // Also attempt logging to local directory fallback if writable
        if let Ok(mut dir) = std::env::current_exe() {
            if cfg!(debug_assertions) {
                dir = std::env::current_dir().unwrap_or_default();
            } else {
                dir.pop();
            }
            let log_dir = dir.join("WatchMark");
            let log_path = log_dir.join("watchmark.log");

            let _ = std::fs::create_dir_all(&log_dir);
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
            {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                let _ = writeln!(file, "[{}] PANIC: {}", timestamp, msg);
            }
        }

        Err(AppError::Panic(msg))
    })
}

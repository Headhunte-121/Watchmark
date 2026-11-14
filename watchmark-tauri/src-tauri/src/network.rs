// WATCHMARK TRACING DIRECTIVE:
// 1. Use tracing::instrument on all public commands/logic blocks.
// 2. Prefer structured logging: info!(action = "...", id = ?, "Message").
// 3. No raw println! allowed.

use crate::error::AppError;
use reqwest::{Client, Error as ReqwestError};
use reqwest_middleware::{
    ClientBuilder, ClientWithMiddleware, Error as MiddlewareError, Middleware, Next,
};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Serialize;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

const USER_AGENT: &str = "WatchMark/2.0 (Windows; Desktop; +https://github.com/Headhunte-121/Watchmark)";

// ---------------------------------------------------------------------------
// TMDB connectivity
// ---------------------------------------------------------------------------
//
// Some internet providers (for example Jio in India) block TMDB while the rest
// of the internet works. We therefore probe a neutral site and TMDB separately
// and keep one shared status that the UI, the background sync and the TMDB
// client all read.

/// Whether TMDB can be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connectivity {
    /// Not checked yet.
    Unknown = 0,
    /// TMDB answers.
    Online = 1,
    /// The internet works but TMDB doesn't (ISP block, DNS filtering, ...).
    TmdbBlocked = 2,
    /// Neither the test sites nor TMDB answer.
    Offline = 3,
}

impl Connectivity {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Connectivity::Online,
            2 => Connectivity::TmdbBlocked,
            3 => Connectivity::Offline,
            _ => Connectivity::Unknown,
        }
    }

    pub fn is_degraded(self) -> bool {
        matches!(self, Connectivity::TmdbBlocked | Connectivity::Offline)
    }

    /// The error a TMDB call should fail with in this state.
    pub fn as_error(self) -> AppError {
        match self {
            Connectivity::Offline => AppError::NetworkOffline,
            _ => AppError::TmdbUnreachable,
        }
    }
}

static CONNECTIVITY: AtomicU8 = AtomicU8::new(Connectivity::Unknown as u8);
static LAST_CHECKED: AtomicI64 = AtomicI64::new(0);

pub fn connectivity() -> Connectivity {
    Connectivity::from_u8(CONNECTIVITY.load(Ordering::SeqCst))
}

/// Stores a new status. Returns the previous one.
pub fn set_connectivity(next: Connectivity) -> Connectivity {
    LAST_CHECKED.store(chrono::Utc::now().timestamp(), Ordering::SeqCst);
    Connectivity::from_u8(CONNECTIVITY.swap(next as u8, Ordering::SeqCst))
}

/// Result of one connectivity check, as sent to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectivityReport {
    pub status: Connectivity,
    /// A neutral test site answered.
    pub internet: bool,
    /// The TMDB API answered.
    pub tmdb: bool,
    /// Unix time of the check.
    pub checked_at: i64,
}

/// Neutral endpoints used to tell "offline" apart from "TMDB blocked".
/// Two providers, so one being blocked or down doesn't look like "offline".
const TEST_SITES: [&str; 2] = [
    "https://www.gstatic.com/generate_204",
    "https://1.1.1.1/cdn-cgi/trace",
];
/// Any HTTP answer from the TMDB API (401 without a key) proves it's reachable.
const TMDB_PROBE: &str = "https://api.themoviedb.org/3/configuration";

fn probe_client() -> Client {
    Client::builder()
        .user_agent(USER_AGENT)
        .https_only(true)
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default()
}

/// Checks the test sites and TMDB concurrently (no retries), updates the
/// shared status and returns the report. Takes at most ~6 seconds.
pub async fn check_connectivity() -> ConnectivityReport {
    let client = probe_client();
    let site = |url: &'static str| {
        let c = client.clone();
        async move { c.get(url).send().await.map(|r| r.status().as_u16() < 500).unwrap_or(false) }
    };
    let tmdb_check = {
        let c = client.clone();
        async move {
            match c.get(TMDB_PROBE).send().await {
                // 200 with a key, 401 without one: both come from TMDB itself.
                Ok(r) => r.status().is_success() || r.status() == reqwest::StatusCode::UNAUTHORIZED,
                Err(_) => false,
            }
        }
    };
    let (a, b, tmdb) = tokio::join!(site(TEST_SITES[0]), site(TEST_SITES[1]), tmdb_check);
    let internet = a || b || tmdb;

    let status = if tmdb {
        Connectivity::Online
    } else if internet {
        Connectivity::TmdbBlocked
    } else {
        Connectivity::Offline
    };
    let previous = set_connectivity(status);
    if previous != status {
        tracing::info!(
            action = "connectivity_changed",
            from = ?previous,
            to = ?status,
            "[NETWORK] 🌐 Connectivity changed: {:?} -> {:?}",
            previous,
            status
        );
    }

    ConnectivityReport {
        status,
        internet,
        tmdb,
        checked_at: LAST_CHECKED.load(Ordering::SeqCst),
    }
}

/// Fails TMDB requests immediately while TMDB is known to be unreachable,
/// instead of letting each one wait for timeouts and retries. The status is
/// re-checked in the background, so requests resume once TMDB is back.
struct TmdbCircuitBreaker;

#[derive(Debug)]
struct TmdbUnavailable(Connectivity);

impl std::fmt::Display for TmdbUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TMDB unavailable ({:?})", self.0)
    }
}

impl std::error::Error for TmdbUnavailable {}

fn is_tmdb_host(url: &reqwest::Url) -> bool {
    url.host_str()
        .map(|h| h.ends_with("themoviedb.org") || h.ends_with("tmdb.org"))
        .unwrap_or(false)
}

#[async_trait::async_trait]
impl Middleware for TmdbCircuitBreaker {
    async fn handle(
        &self,
        req: reqwest::Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        if !is_tmdb_host(req.url()) {
            return next.run(req, extensions).await;
        }

        let state = connectivity();
        if state.is_degraded() {
            return Err(MiddlewareError::Middleware(anyhow::Error::new(TmdbUnavailable(state))));
        }

        let result = next.run(req, extensions).await;
        match &result {
            Ok(_) => {
                if connectivity() != Connectivity::Online {
                    set_connectivity(Connectivity::Online);
                }
            }
            Err(MiddlewareError::Reqwest(e)) if e.is_connect() || e.is_timeout() => {
                // One failure isn't proof of a block; re-check in the background.
                tokio::spawn(async {
                    check_connectivity().await;
                });
            }
            Err(_) => {}
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Shared clients
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct NetworkManager {
    pub external_client: ClientWithMiddleware,
    pub local_client: ClientWithMiddleware,
}

impl NetworkManager {
    pub fn new() -> Self {
        // External Client (TMDB)
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let ext_client = Client::builder()
            .user_agent(USER_AGENT)
            .https_only(true)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to build external reqwest client");

        // The breaker runs first so a known outage skips the retry loop entirely.
        let external_client = ClientBuilder::new(ext_client)
            .with(TmdbCircuitBreaker)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        // Local Client (VLC Heartbeat)
        let loc_client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build local reqwest client");

        let local_client = ClientBuilder::new(loc_client).build();

        Self {
            external_client,
            local_client,
        }
    }

    pub fn handle_error(err: MiddlewareError) -> AppError {
        match err {
            MiddlewareError::Reqwest(e) => Self::map_reqwest_error(e),
            MiddlewareError::Middleware(e) => match e.downcast_ref::<TmdbUnavailable>() {
                Some(TmdbUnavailable(state)) => state.as_error(),
                None => AppError::Custom(format!("Middleware error: {}", e)),
            },
        }
    }

    fn map_reqwest_error(e: ReqwestError) -> AppError {
        if e.is_timeout() {
            return AppError::NetworkTimeout;
        }

        if let Some(status) = e.status() {
            if status == reqwest::StatusCode::FORBIDDEN
                || status == reqwest::StatusCode::UNAUTHORIZED
            {
                return AppError::NetworkBlocked;
            }
        }

        if e.is_connect() {
            // Can be os error 10013 (Permission Denied) or generic offline
            let msg = e.to_string().to_lowercase();
            if msg.contains("os error 10013") || msg.contains("permission denied") {
                return AppError::NetworkBlocked;
            }
            return AppError::NetworkOffline;
        }

        AppError::NetworkError(e)
    }
}

// Global instance to use across the app
lazy_static::lazy_static! {
    pub static ref NETWORK_MANAGER: Arc<NetworkManager> = Arc::new(NetworkManager::new());
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod network_tests;

//! Shared application state injected into `axum` handlers and WebSocket connections.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use boomer_protocol::{GameKindDto, RoomSummaryDto};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::auth::TokenSigner;
use crate::config::ServerConfig;
use crate::ports::{
    GoogleTokenVerifier, HealthCheck, MailSender, MatchRepo, ReportRepo, StatsRepo, UserRepo,
};

/// JSON payload returned by public `GET /status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    /// `"ok"` when healthy, `"degraded"` when the database is unreachable.
    pub status: String,
    /// Server crate version.
    pub version: String,
    /// Seconds elapsed since server startup.
    pub uptime_s: u64,
    /// `"ok"` or `"error"`.
    pub db: String,
    /// Currently connected WebSocket clients.
    pub online: u32,
    /// Active tables across all lounges.
    pub tables: u32,
    /// Active matches currently in progress.
    pub games: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedStatus {
    pub fetched_at: Instant,
    pub healthy: bool,
    pub payload: StatusPayload,
}

/// Shared cloneable handle passed to `axum` routes.
#[derive(Clone)]
pub struct AppState {
    /// Server configuration.
    pub config: Arc<ServerConfig>,
    /// Stateless HMAC token signer.
    pub signer: Arc<TokenSigner>,
    /// User persistence port.
    pub users: Arc<dyn UserRepo>,
    /// Stats & high-score persistence port.
    pub stats: Arc<dyn StatsRepo>,
    /// Match history persistence port.
    pub matches: Arc<dyn MatchRepo>,
    /// Moderation report persistence port.
    pub reports: Arc<dyn ReportRepo>,
    /// Database health check port.
    pub health: Arc<dyn HealthCheck>,
    /// Passwordless email magic-link port.
    pub mailer: Arc<dyn MailSender>,
    /// Google Identity Services token verifier port.
    pub google: Arc<dyn GoogleTokenVerifier>,
    /// Startup instant for `uptimeS`.
    pub started_at: Instant,
    /// Live connected client count.
    pub online: Arc<AtomicU32>,
    /// Live table count (populated by actors in Milestone 5).
    pub tables: Arc<AtomicU32>,
    /// Live active match count (populated by actors in Milestone 5).
    pub games: Arc<AtomicU32>,
    next_guest_num: Arc<AtomicU32>,
    pub(crate) status_cache: Arc<Mutex<Option<CachedStatus>>>,
}

impl AppState {
    /// Constructs [`AppState`] from concrete adapter implementations.
    #[expect(
        clippy::too_many_arguments,
        reason = "Clean Architecture composition root injects each port explicitly"
    )]
    #[must_use]
    pub fn new(
        config: ServerConfig,
        signer: Arc<TokenSigner>,
        users: Arc<dyn UserRepo>,
        stats: Arc<dyn StatsRepo>,
        matches: Arc<dyn MatchRepo>,
        reports: Arc<dyn ReportRepo>,
        health: Arc<dyn HealthCheck>,
        mailer: Arc<dyn MailSender>,
        google: Arc<dyn GoogleTokenVerifier>,
    ) -> Self {
        Self {
            config: Arc::new(config),
            signer,
            users,
            stats,
            matches,
            reports,
            health,
            mailer,
            google,
            started_at: Instant::now(),
            online: Arc::new(AtomicU32::new(0)),
            tables: Arc::new(AtomicU32::new(0)),
            games: Arc::new(AtomicU32::new(0)),
            next_guest_num: Arc::new(AtomicU32::new(1000)),
            status_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Allocates a fresh guest number (`Guest_1000`, `Guest_1001`, ...).
    #[must_use]
    pub fn allocate_guest_num(&self) -> u32 {
        self.next_guest_num.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the baseline lounge rooms announced in `welcome` (extended dynamically in Milestone 5).
    #[must_use]
    pub fn default_rooms(&self) -> Vec<RoomSummaryDto> {
        let online = self.online.load(Ordering::Relaxed);
        vec![
            RoomSummaryDto {
                id: "pool-social-1".to_owned(),
                game: GameKindDto::Pool,
                name: "8-Ball Pool · Social Lounge".to_owned(),
                tier: "social".to_owned(),
                users: online,
            },
            RoomSummaryDto {
                id: "checkers-social-1".to_owned(),
                game: GameKindDto::Checkers,
                name: "Checkers · Social Lounge".to_owned(),
                tier: "social".to_owned(),
                users: 0,
            },
        ]
    }
}

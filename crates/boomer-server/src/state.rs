//! Shared application state injected into `axum` handlers and WebSocket connections.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use boomer_protocol::RoomSummaryDto;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::actors::hub::LobbyHub;
use crate::adapters::anthropic::DisabledAiFriend;
use crate::auth::TokenSigner;
use crate::config::ServerConfig;
use crate::ports::{
    AiFriend, GoogleTokenVerifier, HealthCheck, MailSender, MatchRepo, ReportRepo, StatsRepo,
    UserRepo,
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
    /// Public Google Identity Services OAuth Client ID, when configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_client_id: Option<String>,
    /// Whether the optional Claude AI Friend chat companion is configured.
    pub ai_friend_available: bool,
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
    /// Optional Claude AI Friend chat companion port.
    pub ai_friend: Arc<dyn AiFriend>,
    /// Startup instant for `uptimeS`.
    pub started_at: Instant,
    /// Live connected client count.
    pub online: Arc<AtomicU32>,
    /// Live table count (updated by `TableActor`s).
    pub tables: Arc<AtomicU32>,
    /// Live active match count (updated by `TableActor`s).
    pub games: Arc<AtomicU32>,
    /// Acyclic actor hub managing all lounge rooms and tables.
    pub hub: Arc<LobbyHub>,
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
        let online = Arc::new(AtomicU32::new(0));
        let tables = Arc::new(AtomicU32::new(0));
        let games = Arc::new(AtomicU32::new(0));
        let ai_friend: Arc<dyn AiFriend> = Arc::new(DisabledAiFriend);
        let hub = LobbyHub::new(
            Arc::clone(&online),
            Arc::clone(&tables),
            Arc::clone(&games),
            Arc::clone(&matches),
            Arc::clone(&ai_friend),
        );
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
            ai_friend,
            started_at: Instant::now(),
            online,
            tables,
            games,
            hub,
            next_guest_num: Arc::new(AtomicU32::new(1000)),
            status_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Replaces the [`AiFriend`] port and rebuilds [`LobbyHub`] so lounges reflect availability.
    #[must_use]
    pub fn with_ai_friend(mut self, ai_friend: Arc<dyn AiFriend>) -> Self {
        self.tables.store(0, Ordering::Relaxed);
        self.games.store(0, Ordering::Relaxed);
        self.ai_friend = ai_friend;
        self.hub = LobbyHub::new(
            Arc::clone(&self.online),
            Arc::clone(&self.tables),
            Arc::clone(&self.games),
            Arc::clone(&self.matches),
            Arc::clone(&self.ai_friend),
        );
        self
    }

    /// Replaces the [`LobbyHub`] with custom capacity/auto-open thresholds (for integration tests).
    #[must_use]
    pub fn with_room_limits(mut self, room_capacity: usize, auto_open_threshold: u32) -> Self {
        self.tables.store(0, Ordering::Relaxed);
        self.games.store(0, Ordering::Relaxed);
        self.hub = LobbyHub::with_limits(
            Arc::clone(&self.online),
            Arc::clone(&self.tables),
            Arc::clone(&self.games),
            Arc::clone(&self.matches),
            Arc::clone(&self.ai_friend),
            room_capacity,
            auto_open_threshold,
        );
        self
    }

    /// Allocates a fresh guest number (`Guest_1000`, `Guest_1001`, ...).
    #[must_use]
    pub fn allocate_guest_num(&self) -> u32 {
        self.next_guest_num.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns all currently open lounge rooms and live occupancy counts.
    #[must_use]
    pub fn default_rooms(&self) -> Vec<RoomSummaryDto> {
        self.hub.list_rooms()
    }
}

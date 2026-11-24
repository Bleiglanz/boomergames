//! Clean Architecture server port traits and persistence records.

use std::future::Future;
use std::pin::Pin;

use boomer_domain::GameKind;
use chrono::{DateTime, Utc};

/// Boxed `Send` future alias keeping async port traits object-safe without external proc-macros.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Errors returned by persistence or external identity/mail ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    /// Requested record was not found.
    NotFound,
    /// Unique constraint violated (e.g., display name already taken).
    Conflict(String),
    /// Invalid input supplied to adapter.
    Invalid(String),
    /// Backend/external service failure.
    Unavailable(String),
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid: {msg}"),
            Self::Unavailable(msg) => write!(f, "unavailable: {msg}"),
        }
    }
}

impl std::error::Error for PortError {}

/// Persistent registered user record matching `users` table (§9.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    /// Auto-generated primary key.
    pub id: i64,
    /// Case-insensitive unique email.
    pub email: String,
    /// Optional Google `sub` identifier.
    pub google_sub: Option<String>,
    /// Case-insensitive unique display name.
    pub display_name: String,
    /// Last display name change timestamp (enforces 30-day cooldown).
    pub name_changed_at: Option<DateTime<Utc>>,
    /// Built-in avatar index (`0..16`).
    pub avatar: i16,
    /// Persistent ignored user IDs.
    pub ignored_ids: Vec<i64>,
    /// Moderator privilege flag.
    pub is_moderator: bool,
    /// Optional active mute expiry.
    pub muted_until: Option<DateTime<Utc>>,
    /// Optional active ban expiry.
    pub banned_until: Option<DateTime<Utc>>,
    /// Bumped on ban / global logout to invalidate existing stateless sessions.
    pub session_version: i32,
    /// Member-since timestamp.
    pub created_at: DateTime<Utc>,
}

/// Persistent per-game player rating & win counters (`player_stats` table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStatRecord {
    /// Registered user ID.
    pub user_id: i64,
    /// Game kind.
    pub game: GameKind,
    /// Current Elo rating (starts at 1200).
    pub rating: i32,
    /// Number of rated games played (`< 10` = provisional).
    pub rated_games: i32,
    /// Total human games played.
    pub games: i32,
    /// Total counted human wins (High Score table metric).
    pub wins: i32,
    /// Earliest timestamp when `wins` count was reached (tie-breaker).
    pub wins_reached_at: Option<DateTime<Utc>>,
}

/// Compact match summary row returned for a user's profile (last 20 matches).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchHistoryItem {
    /// Game kind.
    pub game: GameKind,
    /// Opponent display name or bot label.
    pub opponent_name: String,
    /// `"win"`, `"loss"`, or `"draw"` from the queried user's perspective.
    pub result: String,
    /// Completion timestamp.
    pub ended_at: DateTime<Utc>,
}

/// High score table entry joined with user profile info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighscoreEntry {
    /// 1-based rank.
    pub rank: u32,
    /// User ID.
    pub user_id: i64,
    /// Display name.
    pub display_name: String,
    /// Built-in avatar index.
    pub avatar: i16,
    /// Counted human wins.
    pub wins: u32,
    /// Total human games played.
    pub games: u32,
}

/// Verified claims returned by [`GoogleTokenVerifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedGoogleUser {
    /// Stable Google `sub` claim.
    pub sub: String,
    /// Verified email address.
    pub email: String,
    /// Optional suggested display name from profile.
    pub suggested_name: Option<String>,
}

/// Port for registered user persistence.
pub trait UserRepo: Send + Sync {
    /// Fetches a user by primary key.
    fn find_by_id(&self, id: i64) -> BoxFuture<'_, Result<Option<UserRecord>, PortError>>;

    /// Finds an existing user by email or creates a new registered user.
    /// Returns `(user, is_new)`.
    fn find_or_create_by_email(
        &self,
        email: &str,
        google_sub: Option<&str>,
        suggested_name: Option<&str>,
    ) -> BoxFuture<'_, Result<(UserRecord, bool), PortError>>;

    /// Updates display name (checking uniqueness & 30-day cooldown) and/or avatar.
    fn update_account(
        &self,
        user_id: i64,
        new_name: Option<&str>,
        new_avatar: Option<i16>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<UserRecord, PortError>>;

    /// Permanently deletes a user account (cascading `player_stats`, nullifying `matches`/`reports`).
    fn delete_user(&self, user_id: i64) -> BoxFuture<'_, Result<(), PortError>>;

    /// Adds or removes `target_id` from `user_id`'s persisted `ignored_ids` list.
    fn set_ignored(
        &self,
        user_id: i64,
        target_id: i64,
        ignored: bool,
    ) -> BoxFuture<'_, Result<Vec<i64>, PortError>>;

    /// Sets a user's `muted_until` timestamp (moderation tool).
    fn set_mute(
        &self,
        user_id: i64,
        until: Option<DateTime<Utc>>,
    ) -> BoxFuture<'_, Result<(), PortError>>;

    /// Sets a user's `banned_until` timestamp and increments `session_version` to invalidate active tokens.
    fn set_ban(
        &self,
        user_id: i64,
        until: Option<DateTime<Utc>>,
    ) -> BoxFuture<'_, Result<(), PortError>>;
}

/// Port for per-game player ratings and the High Score leaderboard.
pub trait StatsRepo: Send + Sync {
    /// Returns all game stats for `user_id`.
    fn stats_for_user(
        &self,
        user_id: i64,
    ) -> BoxFuture<'_, Result<Vec<PlayerStatRecord>, PortError>>;

    /// Returns Top 100 High Score rows for `game`, or rows around `around_user_id` ("Find me").
    fn highscores(
        &self,
        game: GameKind,
        around_user_id: Option<i64>,
    ) -> BoxFuture<'_, Result<Vec<HighscoreEntry>, PortError>>;
}

/// Atomic persistence request for a completed multiplayer or bot match.
#[derive(Debug, Clone)]
pub struct RecordMatchInput {
    /// Game played.
    pub game: GameKind,
    /// Registered `users.id` in Seat 1 (`None` for guests).
    pub player1_id: Option<i64>,
    /// Registered `users.id` in Seat 2 (`None` for guests or bots).
    pub player2_id: Option<i64>,
    /// Seat 1 display label (used for history when opponent is a guest/bot).
    pub player1_label: String,
    /// Seat 2 display label.
    pub player2_label: String,
    /// Bot difficulty (`Some(1|2|3)`) when playing against a computer bot.
    pub bot_level: Option<i16>,
    /// Winning seat (`Some(1)`, `Some(2)`, or `None` for draw).
    pub winner: Option<i16>,
    /// Short finish reason (`"no_moves"`, `"eight_ball_pocketed"`, `"resign"`, `"timeout"`, ...).
    pub reason: String,
    /// Whether the table was configured as rated (both seats must be registered humans).
    pub rated_requested: bool,
    /// Activity count (Checkers plies or Pool shots) for minimum High Score threshold.
    pub activity_count: u32,
}

/// Rating change recorded for a single player in an atomic match-finish transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRatingChange {
    /// Registered `users.id`.
    pub user_id: i64,
    /// Rating before the match.
    pub old_rating: i32,
    /// Rating after the match.
    pub new_rating: i32,
    /// Updated rated games count.
    pub new_rated_games: i32,
}

/// Updated High Score win count recorded for the match winner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedWinCount {
    /// Registered `users.id`.
    pub user_id: i64,
    /// Total counted human wins for this game.
    pub wins: u32,
}

/// Result returned by [`MatchRepo::record_finished_match`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordMatchOutcome {
    /// Whether the match affected Elo ratings.
    pub rated: bool,
    /// Whether the win incremented High Scores.
    pub counts_for_highscore: bool,
    /// Rating updates applied (0 or 2 entries).
    pub rating_changes: Vec<RecordedRatingChange>,
    /// Updated win counts applied (0 or 1 entry).
    pub win_counts: Vec<RecordedWinCount>,
}

/// Port for finished match history & atomic post-match persistence.
pub trait MatchRepo: Send + Sync {
    /// Returns the most recent `limit` matches involving `user_id`.
    fn recent_matches(
        &self,
        user_id: i64,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<MatchHistoryItem>, PortError>>;

    /// Atomically inserts the `matches` row and upserts `player_stats` (Elo + High Score wins)
    /// enforcing the 5-wins/day/opponent cap, anti-farming decay, and minimum activity threshold.
    fn record_finished_match(
        &self,
        input: RecordMatchInput,
    ) -> BoxFuture<'_, Result<RecordMatchOutcome, PortError>>;
}

/// Port for moderation reports.
pub trait ReportRepo: Send + Sync {
    /// Inserts a moderation report with an optional chat snapshot.
    fn create_report(
        &self,
        reporter_id: Option<i64>,
        target_id: Option<i64>,
        target_name: &str,
        reason: &str,
        chat_log: Option<&str>,
    ) -> BoxFuture<'_, Result<i64, PortError>>;
}

/// Port for checking backing store reachability (used by `GET /status`).
pub trait HealthCheck: Send + Sync {
    /// Returns `Ok(())` when the backing store is reachable.
    fn ping(&self) -> BoxFuture<'_, Result<(), PortError>>;
}

/// Port for sending passwordless magic-link emails.
pub trait MailSender: Send + Sync {
    /// Sends a login magic link to `to_email`.
    fn send_magic_link(
        &self,
        to_email: &str,
        login_url: &str,
    ) -> BoxFuture<'_, Result<(), PortError>>;
}

/// Port for verifying Google Identity Services ID tokens.
pub trait GoogleTokenVerifier: Send + Sync {
    /// Verifies `id_token` and extracts the authenticated user's `sub` and verified email.
    fn verify_id_token(
        &self,
        id_token: &str,
    ) -> BoxFuture<'_, Result<VerifiedGoogleUser, PortError>>;
}

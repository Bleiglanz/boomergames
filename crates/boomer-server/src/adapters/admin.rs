//! Strictly gated `/antonadmin` REST API and anonymous offline-play telemetry endpoint.
//!
//! Security contract:
//! - Every `/antonadmin/api/*` endpoint (except Google sign-in exchange) strictly enforces
//!   Google authentication for [`ADMIN_EMAIL`] (`antonrechenauer@gmail.com`).
//! - Email magic-link tokens, guests, unauthenticated requests, and all other Google accounts
//!   receive `403 Forbidden`.
//! - Zero HTTP cookies are read or set; bearer tokens travel exclusively via `Authorization`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::IntoResponse;
use boomer_protocol::RoomSummaryDto;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::actors::table_actor::TableCmd;
use crate::auth::{ADMIN_EMAIL, ADMIN_TOKEN_TTL_SECS, AuthProvider};
use crate::mapping::{format_user_id, game_kind_to_db, parse_user_id};
use crate::ports::{
    ActivityEventRecord, AdminMatchRecord, AdminReportRecord, AiFriendAdminInfo, NewActivityEvent,
    PlayerStatRecord, UserRecord,
};
use crate::state::AppState;

/// Persistent `site_settings` key storing `"true"` or `"false"` for Maintenance Mode.
pub const SETTING_MAINTENANCE_ENABLED: &str = "maintenance_enabled";

/// Persistent `site_settings` key storing the custom Maintenance Mode banner text.
pub const SETTING_MAINTENANCE_MESSAGE: &str = "maintenance_message";

struct AdminPrincipal {
    email: String,
    user_id: Option<i64>,
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|tok| !tok.is_empty())
}

async fn require_admin(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AdminPrincipal, StatusCode> {
    let Some(token) = extract_bearer(headers) else {
        return Err(StatusCode::FORBIDDEN);
    };

    // 1. Dedicated Google-verified admin token issued by `POST /antonadmin/api/auth/google`.
    if let Ok(claims) = state.signer.verify_admin_google(token)
        && claims.email.eq_ignore_ascii_case(ADMIN_EMAIL)
    {
        let user_id = find_admin_user_id(state).await;
        return Ok(AdminPrincipal {
            email: claims.email,
            user_id,
        });
    }

    // 2. Existing main-site session token ONLY if authenticated via Google AND matching ADMIN_EMAIL.
    if let Ok(claims) = state.signer.verify_session(token)
        && claims.auth_provider == AuthProvider::Google
        && let Ok(Some(user)) = state.users.find_by_id(claims.user_id).await
        && user.session_version == claims.session_version
        && user.google_sub.is_some()
        && user.email.eq_ignore_ascii_case(ADMIN_EMAIL)
        && user.banned_until.is_none_or(|until| until <= Utc::now())
    {
        return Ok(AdminPrincipal {
            email: user.email,
            user_id: Some(user.id),
        });
    }

    Err(StatusCode::FORBIDDEN)
}

async fn find_admin_user_id(state: &AppState) -> Option<i64> {
    state
        .users
        .list_all_users()
        .await
        .ok()
        .and_then(|users| {
            users
                .into_iter()
                .find(|u| u.email.eq_ignore_ascii_case(ADMIN_EMAIL))
        })
        .map(|u| u.id)
}

/// Request body for `POST /antonadmin/api/auth/google`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGoogleAuthRequest {
    /// Google Identity Services JWT credential.
    pub id_token: String,
}

/// Response body for `POST /antonadmin/api/auth/google`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGoogleAuthResponse {
    /// Signed 12-hour Google-verified admin bearer token.
    pub token: String,
    /// Verified Google email address.
    pub email: String,
    /// Token lifetime in seconds.
    pub expires_in_secs: i64,
}

/// Exchanges a Google Identity Services ID token for a short-lived admin bearer token.
/// Returns `403 Forbidden` for every non-admin Google account or invalid token.
pub async fn admin_auth_google_handler(
    State(state): State<AppState>,
    Json(req): Json<AdminGoogleAuthRequest>,
) -> impl IntoResponse {
    let Ok(verified) = state.google.verify_id_token(&req.id_token).await else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "forbidden" })),
        )
            .into_response();
    };
    if !verified.email.eq_ignore_ascii_case(ADMIN_EMAIL) {
        warn!(
            attempted_email = %verified.email,
            "rejected unauthorized Google sign-in attempt on /antonadmin"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "forbidden" })),
        )
            .into_response();
    }

    let Ok(token) = state
        .signer
        .sign_admin_google(&verified.email, &verified.sub)
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let now = Utc::now();
    let user_id = match state
        .users
        .find_or_create_by_email(
            &verified.email,
            Some(&verified.sub),
            verified.suggested_name.as_deref(),
        )
        .await
    {
        Ok((user, _)) => {
            let _ = state.users.record_login(user.id, "google", now).await;
            Some(user.id)
        }
        Err(_) => None,
    };

    state.record_activity_async(NewActivityEvent {
        event_type: "login_google".to_owned(),
        actor_user_id: user_id,
        actor_label: verified.email.clone(),
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some("admin console login".to_owned()),
    });
    info!(email = %verified.email, "admin authenticated via Google on /antonadmin");

    (
        StatusCode::OK,
        Json(AdminGoogleAuthResponse {
            token,
            email: verified.email,
            expires_in_secs: ADMIN_TOKEN_TTL_SECS,
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminServerStats {
    version: String,
    uptime_s: u64,
    db_ok: bool,
    maintenance: bool,
    maintenance_message: String,
    online: u32,
    tables: u32,
    games: u32,
    draining: bool,
    total_users: usize,
    total_matches: u64,
    open_reports: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminActiveTable {
    table_id: String,
    room_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminOnlineSession {
    wire_id: String,
    kind: String,
    user_id: Option<i64>,
    display_name: String,
    email: Option<String>,
    active_table: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserStatDto {
    game: String,
    rating: i32,
    rated_games: i32,
    games: i32,
    wins: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserDto {
    id: i64,
    wire_id: String,
    email: String,
    display_name: String,
    google_linked: bool,
    is_moderator: bool,
    muted_until: Option<DateTime<Utc>>,
    banned_until: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
    last_login_provider: Option<String>,
    online: bool,
    stats: Vec<AdminUserStatDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminActivityDto {
    id: i64,
    event_type: String,
    actor_user_id: Option<i64>,
    actor_label: String,
    auth_provider: Option<String>,
    game: Option<String>,
    details: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminMatchDto {
    id: i64,
    game: String,
    player1_label: String,
    player2_label: String,
    bot_level: Option<i16>,
    winner: Option<i16>,
    end_reason: String,
    rated: bool,
    ended_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminReportDto {
    id: i64,
    reporter_name: Option<String>,
    reported_id: Option<i64>,
    reported_name: String,
    reason: String,
    chat_log: Option<String>,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiFriendDto {
    configured: bool,
    model: String,
    calls_used: u32,
    max_calls: u32,
    prompt_file: Option<String>,
    active_prompt: String,
    using_custom_file: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminOverviewResponse {
    admin_email: String,
    server: AdminServerStats,
    rooms: Vec<RoomSummaryDto>,
    active_tables: Vec<AdminActiveTable>,
    online_sessions: Vec<AdminOnlineSession>,
    users: Vec<AdminUserDto>,
    activity: Vec<AdminActivityDto>,
    recent_matches: Vec<AdminMatchDto>,
    reports: Vec<AdminReportDto>,
    ai_friend: AdminAiFriendDto,
}

/// `GET /antonadmin/api/overview` — returns full live + historical admin telemetry.
pub async fn admin_overview_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };

    let db_ok = state.health.ping().await.is_ok();
    let users_raw = state.users.list_all_users().await.unwrap_or_default();
    let stats_raw = state.stats.all_stats().await.unwrap_or_default();
    let activity_raw = state.admin.recent_activity(200).await.unwrap_or_default();
    let matches_raw = state
        .matches
        .recent_all_matches(100)
        .await
        .unwrap_or_default();
    let total_matches = state.matches.total_matches().await.unwrap_or(0);
    let reports_raw = state.reports.list_reports(100).await.unwrap_or_default();
    let ai_info = state.ai_friend.admin_info().await;

    let mut stats_by_user: HashMap<i64, Vec<PlayerStatRecord>> = HashMap::new();
    for st in stats_raw {
        stats_by_user.entry(st.user_id).or_default().push(st);
    }
    let users_by_id: HashMap<i64, &UserRecord> = users_raw.iter().map(|u| (u.id, u)).collect();

    let connected_ids = state.hub.connected_wire_ids();
    let online_sessions = build_online_sessions(&connected_ids, &users_by_id, &state);
    let users = build_admin_users(&users_raw, &connected_ids, &mut stats_by_user);

    let open_reports = reports_raw
        .iter()
        .filter(|r| r.resolved_at.is_none())
        .count();

    let resp = AdminOverviewResponse {
        admin_email: principal.email,
        server: AdminServerStats {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            uptime_s: state.started_at.elapsed().as_secs(),
            db_ok,
            maintenance: state.is_maintenance(),
            maintenance_message: state.maintenance_message(),
            online: state.online.load(Ordering::Relaxed),
            tables: state.tables.load(Ordering::Relaxed),
            games: state.games.load(Ordering::Relaxed),
            draining: state.hub.is_draining(),
            total_users: users.len(),
            total_matches,
            open_reports,
        },
        rooms: state.hub.list_rooms(),
        active_tables: state
            .hub
            .active_tables()
            .into_iter()
            .map(|(table_id, room_id)| AdminActiveTable { table_id, room_id })
            .collect(),
        online_sessions,
        users,
        activity: activity_raw.into_iter().map(map_activity_dto).collect(),
        recent_matches: matches_raw.into_iter().map(map_match_dto).collect(),
        reports: reports_raw.into_iter().map(map_report_dto).collect(),
        ai_friend: map_ai_dto(ai_info),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

fn build_online_sessions(
    connected_ids: &[String],
    users_by_id: &HashMap<i64, &UserRecord>,
    state: &AppState,
) -> Vec<AdminOnlineSession> {
    connected_ids
        .iter()
        .map(|wire_id| {
            let active_table = state.hub.user_table(wire_id);
            if let Some(uid) = parse_user_id(wire_id)
                && let Some(u) = users_by_id.get(&uid)
            {
                AdminOnlineSession {
                    wire_id: wire_id.clone(),
                    kind: "registered".to_owned(),
                    user_id: Some(uid),
                    display_name: u.display_name.clone(),
                    email: Some(u.email.clone()),
                    active_table,
                }
            } else {
                let guest_label = wire_id
                    .strip_prefix('g')
                    .map_or_else(|| wire_id.clone(), |num| format!("Guest_{num}"));
                AdminOnlineSession {
                    wire_id: wire_id.clone(),
                    kind: "guest".to_owned(),
                    user_id: None,
                    display_name: guest_label,
                    email: None,
                    active_table,
                }
            }
        })
        .collect()
}

fn build_admin_users(
    users_raw: &[UserRecord],
    connected_ids: &[String],
    stats_by_user: &mut HashMap<i64, Vec<PlayerStatRecord>>,
) -> Vec<AdminUserDto> {
    users_raw
        .iter()
        .map(|u| {
            let wire_id = format_user_id(u.id);
            let online = connected_ids.iter().any(|id| id == &wire_id);
            let user_stats = stats_by_user
                .remove(&u.id)
                .unwrap_or_default()
                .into_iter()
                .map(|s| AdminUserStatDto {
                    game: game_kind_to_db(s.game).to_owned(),
                    rating: s.rating,
                    rated_games: s.rated_games,
                    games: s.games,
                    wins: s.wins,
                })
                .collect();
            AdminUserDto {
                id: u.id,
                wire_id,
                email: u.email.clone(),
                display_name: u.display_name.clone(),
                google_linked: u.google_sub.is_some(),
                is_moderator: u.is_moderator,
                muted_until: u.muted_until,
                banned_until: u.banned_until,
                created_at: u.created_at,
                last_login_at: u.last_login_at,
                last_login_provider: u.last_login_provider.clone(),
                online,
                stats: user_stats,
            }
        })
        .collect()
}

fn map_activity_dto(rec: ActivityEventRecord) -> AdminActivityDto {
    AdminActivityDto {
        id: rec.id,
        event_type: rec.event_type,
        actor_user_id: rec.actor_user_id,
        actor_label: rec.actor_label,
        auth_provider: rec.auth_provider,
        game: rec.game,
        details: rec.details,
        created_at: rec.created_at,
    }
}

fn map_match_dto(rec: AdminMatchRecord) -> AdminMatchDto {
    AdminMatchDto {
        id: rec.id,
        game: game_kind_to_db(rec.game).to_owned(),
        player1_label: rec.player1_label,
        player2_label: rec.player2_label,
        bot_level: rec.bot_level,
        winner: rec.winner,
        end_reason: rec.end_reason,
        rated: rec.rated,
        ended_at: rec.ended_at,
    }
}

fn map_report_dto(rec: AdminReportRecord) -> AdminReportDto {
    AdminReportDto {
        id: rec.id,
        reporter_name: rec.reporter_name,
        reported_id: rec.reported_id,
        reported_name: rec.reported_name,
        reason: rec.reason,
        chat_log: rec.chat_log,
        created_at: rec.created_at,
        resolved_at: rec.resolved_at,
    }
}

fn map_ai_dto(info: AiFriendAdminInfo) -> AdminAiFriendDto {
    AdminAiFriendDto {
        configured: info.configured,
        model: info.model,
        calls_used: info.calls_used,
        max_calls: info.max_calls,
        prompt_file: info.prompt_file,
        active_prompt: info.active_prompt,
        using_custom_file: info.using_custom_file,
    }
}

/// Request body for `POST /antonadmin/api/maintenance`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceRequest {
    /// `true` to put the site into Maintenance Mode; `false` to resume normal operation.
    pub enabled: bool,
    /// Optional visitor-facing maintenance notice.
    #[serde(default)]
    pub message: Option<String>,
    /// When enabling maintenance, whether to disconnect all active WebSocket clients immediately.
    #[serde(default = "default_true")]
    pub disconnect_clients: bool,
}

const fn default_true() -> bool {
    true
}

/// `POST /antonadmin/api/maintenance` — starts or stops Maintenance Mode across the whole site.
pub async fn admin_maintenance_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<MaintenanceRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };

    state.set_maintenance(req.enabled, req.message.as_deref());
    let active_msg = state.maintenance_message();
    let _ = state
        .admin
        .set_setting(
            SETTING_MAINTENANCE_ENABLED,
            if req.enabled { "true" } else { "false" },
        )
        .await;
    let _ = state
        .admin
        .set_setting(SETTING_MAINTENANCE_MESSAGE, &active_msg)
        .await;
    state.clear_status_cache().await;

    let disconnected_clients = if req.enabled && req.disconnect_clients {
        let _ = state
            .hub
            .broadcast_system_notice(&format!("Maintenance mode enabled: {active_msg}"));
        state.hub.kick_all_connected_clients()
    } else {
        0
    };

    let detail = if req.enabled {
        format!("enabled maintenance mode (kicked {disconnected_clients} clients)")
    } else {
        "disabled maintenance mode (site live)".to_owned()
    };
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(detail),
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "maintenance": req.enabled,
            "maintenanceMessage": active_msg,
            "disconnectedClients": disconnected_clients,
        })),
    )
        .into_response()
}

/// Request body for `POST /antonadmin/api/broadcast`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastRequest {
    /// Announcement text sent to all connected lounge/table users.
    pub message: String,
}

/// `POST /antonadmin/api/broadcast` — broadcasts a system announcement to all connected players.
pub async fn admin_broadcast_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<BroadcastRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    let trimmed = req.message.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 300 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "message must be 1..=300 chars" })),
        )
            .into_response();
    }
    let recipients = state
        .hub
        .broadcast_system_notice(&format!("📢 Admin: {trimmed}"));
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!("broadcast to {recipients} users: {trimmed}")),
    });
    (
        StatusCode::OK,
        Json(serde_json::json!({ "recipients": recipients })),
    )
        .into_response()
}

/// Request body for `POST /antonadmin/api/users/mute`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMuteRequest {
    /// Target user ID.
    pub user_id: i64,
    /// Duration in minutes (`0` = unmute).
    pub minutes: u32,
}

/// `POST /antonadmin/api/users/mute` — mutes or unmutes a registered user.
pub async fn admin_mute_user_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminMuteRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    let until = if req.minutes == 0 {
        None
    } else {
        Some(Utc::now() + Duration::minutes(i64::from(req.minutes)))
    };
    if state.users.set_mute(req.user_id, until).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(if req.minutes == 0 {
            format!("unmuted user u{}", req.user_id)
        } else {
            format!("muted user u{} for {}m", req.user_id, req.minutes)
        }),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// Request body for `POST /antonadmin/api/users/ban`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminBanRequest {
    /// Target user ID.
    pub user_id: i64,
    /// Duration in hours (`0` = unban).
    pub hours: u32,
}

/// `POST /antonadmin/api/users/ban` — bans or unbans a registered user (blocking self-ban).
pub async fn admin_ban_user_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminBanRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    if let Ok(Some(target)) = state.users.find_by_id(req.user_id).await
        && target.email.eq_ignore_ascii_case(ADMIN_EMAIL)
        && req.hours > 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "cannot ban admin account" })),
        )
            .into_response();
    }
    let until = if req.hours == 0 {
        None
    } else {
        Some(Utc::now() + Duration::hours(i64::from(req.hours)))
    };
    if state.users.set_ban(req.user_id, until).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if until.is_some() {
        state
            .hub
            .force_disconnect_user(&format_user_id(req.user_id));
    }
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(if req.hours == 0 {
            format!("unbanned user u{}", req.user_id)
        } else {
            format!("banned user u{} for {}h", req.user_id, req.hours)
        }),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// Request body for `POST /antonadmin/api/users/moderator`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminModeratorRequest {
    /// Target user ID.
    pub user_id: i64,
    /// Desired moderator flag.
    pub is_moderator: bool,
}

/// `POST /antonadmin/api/users/moderator` — toggles moderator status for a registered user.
pub async fn admin_moderator_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminModeratorRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    if state
        .users
        .set_moderator(req.user_id, req.is_moderator)
        .await
        .is_err()
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!(
            "set moderator={} on user u{}",
            req.is_moderator, req.user_id
        )),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// Request body for `POST /antonadmin/api/users/delete`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminDeleteUserRequest {
    /// Target user ID.
    pub user_id: i64,
}

/// `POST /antonadmin/api/users/delete` — permanently deletes a non-admin user account.
pub async fn admin_delete_user_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminDeleteUserRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    if let Ok(Some(target)) = state.users.find_by_id(req.user_id).await
        && target.email.eq_ignore_ascii_case(ADMIN_EMAIL)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "cannot delete admin account" })),
        )
            .into_response();
    }
    if state.users.delete_user(req.user_id).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    state
        .hub
        .force_disconnect_user(&format_user_id(req.user_id));
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!("deleted user u{}", req.user_id)),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// Request body for `POST /antonadmin/api/tables/close`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminCloseTableRequest {
    /// Active table ID (`"t12"`).
    pub table_id: String,
}

/// `POST /antonadmin/api/tables/close` — forcibly closes an active table.
pub async fn admin_close_table_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminCloseTableRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    let Some(table) = state.hub.find_table(&req.table_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    table.try_send(TableCmd::ForceClose);
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!("closed table {}", req.table_id)),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// Request body for `POST /antonadmin/api/reports/resolve`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminResolveReportRequest {
    /// Report primary key.
    pub report_id: i64,
}

/// `POST /antonadmin/api/reports/resolve` — marks a moderation report resolved.
pub async fn admin_resolve_report_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminResolveReportRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    if state
        .reports
        .resolve_report(req.report_id, principal.user_id)
        .await
        .is_err()
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!("resolved report #{}", req.report_id)),
    });
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// `POST /antonadmin/api/ai/reset` — resets the in-memory AI Friend safety call counter.
pub async fn admin_ai_reset_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    let previous = state.ai_friend.reset_call_counter();
    state.record_activity_async(NewActivityEvent {
        event_type: "admin_action".to_owned(),
        actor_user_id: principal.user_id,
        actor_label: principal.email,
        auth_provider: Some("google".to_owned()),
        game: None,
        details: Some(format!("reset AI Friend call counter (was {previous})")),
    });
    (
        StatusCode::OK,
        Json(serde_json::json!({ "previousCallsUsed": previous })),
    )
        .into_response()
}

/// Request body for `POST /antonadmin/api/ai/prompt`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAiPromptRequest {
    /// Replacement UTF-8 system prompt text persisted into the configured prompt file.
    pub prompt: String,
}

/// `POST /antonadmin/api/ai/prompt` — writes an updated AI Friend system prompt to disk.
pub async fn admin_ai_prompt_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AdminAiPromptRequest>,
) -> impl IntoResponse {
    let principal = match require_admin(&headers, &state).await {
        Ok(p) => p,
        Err(code) => {
            return (code, Json(serde_json::json!({ "error": "forbidden" }))).into_response();
        }
    };
    match state.ai_friend.save_system_prompt(&req.prompt).await {
        Ok(()) => {
            state.record_activity_async(NewActivityEvent {
                event_type: "admin_action".to_owned(),
                actor_user_id: principal.user_id,
                actor_label: principal.email,
                auth_provider: Some("google".to_owned()),
                game: None,
                details: Some(format!(
                    "updated AI Friend system prompt ({} chars)",
                    req.prompt.trim().chars().count()
                )),
            });
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

/// Lightweight zero-PII beacon sent when someone starts a local/offline game.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePlayTelemetryRequest {
    /// `"pool"`, `"checkers"`, or `"chess"`.
    pub game: String,
    /// `"hotseat"`, `"vs_bot"`, or `"free_practice"`.
    pub mode: String,
    /// Optional bot level (`"easy"`, `"medium"`, `"hard"`).
    #[serde(default)]
    pub level: Option<String>,
}

/// `POST /api/telemetry/offline` — records anonymous offline-game start timestamps (zero PII/cookies).
pub async fn offline_play_telemetry_handler(
    State(state): State<AppState>,
    Json(req): Json<OfflinePlayTelemetryRequest>,
) -> impl IntoResponse {
    let game = match req.game.as_str() {
        "pool" | "checkers" | "chess" => req.game,
        _ => return StatusCode::BAD_REQUEST,
    };
    let mode = match req.mode.as_str() {
        "hotseat" | "vs_bot" | "free_practice" => req.mode,
        _ => return StatusCode::BAD_REQUEST,
    };
    let detail = match (mode.as_str(), req.level.as_deref()) {
        ("vs_bot", Some(lvl @ ("easy" | "medium" | "hard"))) => format!("vs_bot ({lvl})"),
        _ => mode,
    };
    state.record_activity_async(NewActivityEvent {
        event_type: "offline_play".to_owned(),
        actor_user_id: None,
        actor_label: "Anonymous (Offline)".to_owned(),
        auth_provider: None,
        game: Some(game),
        details: Some(detail),
    });
    StatusCode::NO_CONTENT
}

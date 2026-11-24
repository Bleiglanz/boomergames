//! Standalone wire protocol DTOs for Milestone 5 lounges, tables, chat, and invites.

use serde::{Deserialize, Serialize};

use crate::auth::UserDto;
use crate::checkers::CheckersClockModeDto;
use crate::envelope::{GameKindDto, SeatDto};
use crate::pool::{AimAidLevelDto, BallInHandRuleDto};

/// Access control mode of a multiplayer table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TableVisibilityDto {
    /// Visible in the lobby; anyone may join or watch.
    #[default]
    Public,
    /// Visible in the lobby; anyone may watch, seated play requires invite/approval.
    Protected,
    /// Hidden or invite-only table; only invited users may join or watch.
    Private,
}

/// Lifecycle phase of a multiplayer table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TablePhaseDto {
    /// Waiting for seats and Ready clicks.
    #[default]
    Waiting,
    /// Match currently in progress.
    Playing,
    /// Match finished; awaiting rematch or players leaving.
    Over,
}

/// Role requested when joining a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableRoleDto {
    /// Take an open player seat.
    Player,
    /// Join the spectator gallery.
    Spectator,
}

/// Scope distinguishing lobby room chat from table-isolated chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatScopeDto {
    /// Lounge room chat.
    Lobby,
    /// Isolated table chat.
    Table,
}

/// Reason a direct table invite closed without starting a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteCloseReasonDto {
    /// Invitee declined.
    Declined,
    /// 30-second invite window elapsed.
    Expired,
    /// Inviter left or cancelled.
    Cancelled,
}

/// Reason a client was removed from a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickReasonDto {
    /// Booted by the table host (triggers 5-minute rejoin block).
    Booted,
    /// Table was closed by the host or a moderator.
    TableClosed,
}

/// Optional per-game rule overrides attached to [`TableSettingsDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableGameOptsDto {
    /// Pool aiming aid tier selected by the table host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aim: Option<AimAidLevelDto>,
    /// Pool call-pocket on 8-ball toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_eight: Option<bool>,
    /// Pool ball-in-hand placement rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bih: Option<BallInHandRuleDto>,
    /// Checkers clock mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<CheckersClockModeDto>,
    /// Checkers total game clock minutes (when `clock == Game`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_min: Option<u32>,
}

/// Host-configurable settings for a multiplayer table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSettingsDto {
    /// Table access mode (`public`, `protected`, `private`).
    #[serde(rename = "type", default)]
    pub visibility: TableVisibilityDto,
    /// Whether the match affects Elo ratings (automatically disabled for guests/bots).
    #[serde(default = "default_true")]
    pub rated: bool,
    /// Shot/move timer in seconds (`0` = unlimited).
    #[serde(default = "default_timer_secs")]
    pub timer: u32,
    /// Whether spectator chat is silenced at this table.
    #[serde(default)]
    pub mute_spectators: bool,
    /// Optional game-specific options.
    #[serde(default)]
    pub opts: TableGameOptsDto,
}

const fn default_true() -> bool {
    true
}

const fn default_timer_secs() -> u32 {
    30
}

impl Default for TableSettingsDto {
    fn default() -> Self {
        Self {
            visibility: TableVisibilityDto::Public,
            rated: true,
            timer: default_timer_secs(),
            mute_spectators: false,
            opts: TableGameOptsDto::default(),
        }
    }
}

/// Compact row describing a table in the lobby table list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummaryDto {
    /// Stable table identifier (e.g., `"t29"`).
    pub id: String,
    /// Human-friendly sequential table number (`29`).
    pub no: u32,
    /// Game played at this table.
    pub game: GameKindDto,
    /// Seat 1 and Seat 2 occupants.
    pub seats: [Option<UserDto>; 2],
    /// Table access mode.
    #[serde(rename = "type")]
    pub visibility: TableVisibilityDto,
    /// Whether the table is rated.
    pub rated: bool,
    /// Per-turn timer in seconds (`0` = unlimited).
    pub timer: u32,
    /// Game-specific rule options.
    pub opts: TableGameOptsDto,
    /// Current spectator count.
    pub watchers: u32,
    /// Table lifecycle phase.
    pub phase: TablePhaseDto,
}

/// Single chat or system message line in lobby or table chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLineDto {
    /// Sender wire ID (`None` for system announcements).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Sender display name (`None` for system announcements).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Message text (≤ 300 chars, profanity-filtered).
    pub text: String,
    /// UTC Unix timestamp in milliseconds.
    pub ts: i64,
    /// `true` for server-generated system lines.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sys: bool,
}

/// Complete lounge room snapshot delivered on `lobby.join`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbySnapshotDto {
    /// Joined room ID (e.g., `"pool-social-1"`).
    pub room: String,
    /// Game hosted by this room.
    pub game: GameKindDto,
    /// Caller's own [`UserDto`] in this room's game context.
    pub me: UserDto,
    /// Live users currently in this room.
    pub users: Vec<UserDto>,
    /// Active tables in this room.
    pub tables: Vec<TableSummaryDto>,
    /// Recent room chat lines (last 50).
    pub chat: Vec<ChatLineDto>,
    /// Total users online across the server.
    pub online: u32,
    /// Wire IDs currently ignored by this user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
}

/// Complete table state snapshot delivered on join, create, Play Now, invite accept, or reconnect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSnapshotDto {
    /// Table ID (`"t29"`).
    pub id: String,
    /// Sequential table number.
    pub no: u32,
    /// Game played at this table.
    pub game: GameKindDto,
    /// Owning room ID.
    pub room: String,
    /// Current host wire ID.
    pub host: String,
    /// Current table settings.
    pub settings: TableSettingsDto,
    /// Seat 1 and Seat 2 occupants.
    pub seats: [Option<UserDto>; 2],
    /// Ready toggle states for Seat 1 and Seat 2.
    pub ready: [bool; 2],
    /// Connected spectators.
    pub watchers: Vec<UserDto>,
    /// Lifecycle phase.
    pub phase: TablePhaseDto,
    /// Recent table chat lines (last 50).
    pub chat: Vec<ChatLineDto>,
    /// Active turn number (`0` before game start).
    pub turn: u32,
    /// Active seat (`None` before game start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_seat: Option<SeatDto>,
    /// Milliseconds remaining on the active turn clock.
    pub ms_left: u64,
}

/// Incremental table state update broadcast when seats, ready flags, settings, watchers, or phase change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableUpdateDto {
    /// Table ID.
    pub id: String,
    /// Current host wire ID.
    pub host: String,
    /// Current table settings.
    pub settings: TableSettingsDto,
    /// Seat 1 and Seat 2 occupants.
    pub seats: [Option<UserDto>; 2],
    /// Ready flags.
    pub ready: [bool; 2],
    /// Connected spectators.
    pub watchers: Vec<UserDto>,
    /// Lifecycle phase.
    pub phase: TablePhaseDto,
}

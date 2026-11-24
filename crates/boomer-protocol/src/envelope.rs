//! Versioned JSON message envelopes (`{ "t": "<type>", "d": { ... } }`).

use serde::{Deserialize, Serialize};

use crate::auth::{HighscoreRowDto, ProfileGameStatDto, ProfileMatchDto, RoomSummaryDto, UserDto};
use crate::checkers::{CheckersSettingsDto, CheckersStateDto};
use crate::lobby::{
    ChatLineDto, ChatScopeDto, GameOverDto, InviteCloseReasonDto, KickReasonDto, LobbySnapshotDto,
    TableRoleDto, TableSettingsDto, TableSnapshotDto, TableSummaryDto, TableUpdateDto,
};
use crate::pool::{PoolBallDto, PoolSettingsDto, PoolShotDto, PoolStateDto};

/// Current wire protocol version validated during `hello` handshake.
pub const PROTOCOL_VERSION: u32 = 1;

/// Wire representation of supported games.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameKindDto {
    /// English draughts / American checkers.
    Checkers,
    /// Standard WPA/BCA 8-ball pool.
    Pool,
}

/// Wire representation of a player seat (`1` or `2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeatDto {
    /// Seat 1.
    #[serde(rename = "1")]
    One,
    /// Seat 2.
    #[serde(rename = "2")]
    Two,
}

/// Wire representation of computer opponent difficulty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotLevelDto {
    /// Level 1 (Easy).
    Easy,
    /// Level 2 (Medium).
    Medium,
    /// Level 3 (Hard).
    Hard,
}

/// Wire representation of offline single-player session modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalModeDto {
    /// Local pass-and-play (hotseat).
    Hotseat,
    /// Single-player versus computer AI.
    VsBot {
        /// Selected difficulty tier.
        level: BotLevelDto,
    },
    /// Solo open-rack practice (Pool only).
    FreePractice,
}

/// Transport session mode reported by `welcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKindDto {
    /// Offline client-only session (Step 1).
    Local,
    /// Authoritative remote WebSocket session (Step 2).
    Remote,
}

/// Standard protocol error codes from the Boomergames specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Action attempted while not the active turn player.
    NotYourTurn,
    /// Turn counter did not match server state.
    StaleTurn,
    /// Move or shot violated game rules.
    IllegalAction,
    /// Host-only control attempted by a non-host.
    NotHost,
    /// Seat already occupied.
    TableFull,
    /// Operation disallowed in the current state.
    NotAllowed,
    /// User is muted.
    Muted,
    /// Rate limit exceeded.
    RateLimited,
    /// Malformed or out-of-range payload.
    Invalid,
    /// Client protocol version mismatch; client should reload.
    UpgradeRequired,
    /// Account or session banned.
    Banned,
}

/// Messages sent from client to transport (`LocalSessionAdapter` or `boomer-server`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ClientMessage {
    /// Initial protocol handshake.
    #[serde(rename = "hello")]
    Hello {
        /// Protocol version.
        v: u32,
        /// Optional signed session token (Step 2).
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<String>,
        /// Optional guest reconnect token (Step 2).
        #[serde(skip_serializing_if = "Option::is_none")]
        guest: Option<String>,
    },
    /// Start an offline single-player session (Step 1 local transport).
    #[serde(rename = "local.start")]
    LocalStart {
        /// Selected game.
        game: GameKindDto,
        /// Selected offline mode.
        mode: LocalModeDto,
    },
    /// Leave the active offline game and return to the home picker.
    #[serde(rename = "local.leave")]
    LocalLeave {},
    /// Submit a Checkers move path (`1..=32` notation).
    #[serde(rename = "checkers.move")]
    CheckersMove {
        /// Expected turn number.
        turn: u32,
        /// Ordered sequence of `1..=32` dark squares.
        path: Vec<u8>,
    },
    /// Update Checkers clock settings (applies immediately to a fresh match).
    #[serde(rename = "checkers.settings")]
    CheckersSettings {
        /// Updated clock settings.
        settings: CheckersSettingsDto,
    },
    /// Submit a Pool shot.
    #[serde(rename = "pool.shoot")]
    PoolShoot {
        /// Shot parameters.
        shot: PoolShotDto,
    },
    /// Place the cue ball during ball-in-hand.
    #[serde(rename = "pool.place_cue")]
    PoolPlaceCue {
        /// Target X coordinate in metres.
        x: f64,
        /// Target Y coordinate in metres.
        y: f64,
    },
    /// Update Pool table rules/settings.
    #[serde(rename = "pool.settings")]
    PoolSettings {
        /// Updated settings.
        settings: PoolSettingsDto,
    },
    /// Re-rack the Pool table (Free Practice).
    #[serde(rename = "pool.rerack")]
    PoolRerack {},
    /// Resign the active match.
    #[serde(rename = "resign")]
    Resign {},
    /// Offer a draw in Checkers.
    #[serde(rename = "draw.offer")]
    DrawOffer {},
    /// Accept or decline a pending draw offer in Checkers.
    #[serde(rename = "draw.answer")]
    DrawAnswer {
        /// `true` to accept the draw, `false` to decline.
        accept: bool,
    },
    /// Request a rematch (alternates starting colors/breaker).
    #[serde(rename = "rematch")]
    Rematch {},
    /// Request a passwordless email magic link (Step 2).
    #[serde(rename = "auth.email")]
    AuthEmail {
        /// Recipient email address.
        email: String,
    },
    /// Complete passwordless sign-in using a signed magic-link token (Step 2).
    #[serde(rename = "auth.magic")]
    AuthMagic {
        /// Signed magic-link token from `/#login=...`.
        token: String,
    },
    /// Sign in with a Google Identity Services ID token (Step 2).
    #[serde(rename = "auth.google")]
    AuthGoogle {
        /// Signed JWT ID token from Google.
        #[serde(rename = "idToken")]
        id_token: String,
    },
    /// Log out of the registered account and revert to a guest session.
    #[serde(rename = "auth.logout")]
    AuthLogout {},
    /// Update display name and/or built-in avatar index.
    #[serde(rename = "account.update")]
    AccountUpdate {
        /// Optional new display name (unique, profanity-filtered, 30-day cooldown).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Optional built-in avatar index (`0..16`).
        #[serde(skip_serializing_if = "Option::is_none")]
        avatar: Option<u8>,
    },
    /// Permanently delete the authenticated account (GDPR NFR-8).
    #[serde(rename = "account.delete")]
    AccountDelete {},
    /// Fetch a registered user's profile and last 20 matches.
    #[serde(rename = "profile.get")]
    ProfileGet {
        /// Target user wire ID (`"u123"`).
        #[serde(rename = "userId")]
        user_id: String,
    },
    /// Fetch the Top 100 High Score table (or rows around the caller).
    #[serde(rename = "highscores.get")]
    HighscoresGet {
        /// Game leaderboard to fetch.
        game: GameKindDto,
        /// When `true`, center results around the caller ("Find me").
        #[serde(default)]
        around: bool,
    },
    /// Join (or switch into) a lounge room.
    #[serde(rename = "lobby.join")]
    LobbyJoin {
        /// Room identifier (e.g., `"pool-social-1"`).
        room: String,
    },
    /// Send a chat line to the active table or lobby room (≤ 300 chars, 1 msg/s).
    #[serde(rename = "chat.send")]
    ChatSend {
        /// Message text.
        text: String,
        /// Optional explicit target (`lobby` or `table`); defaults to `table` when seated/watching.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<ChatScopeDto>,
    },
    /// Create a new table in the active lounge and take Seat 1 as host.
    #[serde(rename = "table.create")]
    TableCreate(TableSettingsDto),
    /// Join an existing table as a player or spectator (works via lobby list or `/#table=<id>`).
    #[serde(rename = "table.join")]
    TableJoin {
        /// Target table ID (e.g., `"t1"`).
        #[serde(rename = "tableId")]
        table_id: String,
        /// Requested role (`player` or `spectator`).
        #[serde(rename = "as")]
        role: TableRoleDto,
    },
    /// Leave the active table and return to the lounge.
    #[serde(rename = "table.leave")]
    TableLeave {},
    /// Quick matchmaking (human-first with ±200 Elo widening & 20s wait, or immediate `vsBot`).
    #[serde(rename = "play.now")]
    PlayNow {
        /// Optional computer opponent difficulty for immediate bot play.
        #[serde(rename = "vsBot", default, skip_serializing_if = "Option::is_none")]
        vs_bot: Option<BotLevelDto>,
    },
    /// Send a 30-second direct invite to another available user in the room.
    #[serde(rename = "invite.send")]
    InviteSend {
        /// Target user wire ID.
        to: String,
        /// Optional existing table ID to invite into.
        #[serde(rename = "tableId", default, skip_serializing_if = "Option::is_none")]
        table_id: Option<String>,
        /// Optional settings for creating a new table on accept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<TableSettingsDto>,
    },
    /// Accept or decline a pending direct invite.
    #[serde(rename = "invite.answer")]
    InviteAnswer {
        /// Pending invite identifier.
        #[serde(rename = "inviteId")]
        invite_id: String,
        /// `true` to accept and seat immediately, `false` to decline.
        accept: bool,
    },
    /// Persist ignore/mute preference for another user (`u123` or `g4821`).
    #[serde(rename = "ignore.set")]
    IgnoreSet {
        /// Target user wire ID.
        #[serde(rename = "userId")]
        user_id: String,
        /// `true` to ignore, `false` to unignore.
        on: bool,
    },
    /// Toggle Ready / Start at the active table.
    #[serde(rename = "ready")]
    Ready {},
    /// Update pre-game table settings (host only, `waiting` phase only).
    #[serde(rename = "settings.set")]
    SettingsSet(TableSettingsDto),
    /// Move from spectator into an open seat.
    #[serde(rename = "seat.take")]
    SeatTake {
        /// Desired seat (`1` or `2`).
        seat: SeatDto,
    },
    /// Stand up from a seat and become a spectator at the same table.
    #[serde(rename = "seat.leave")]
    SeatLeave {},
    /// Boot a user from the table with a 5-minute rejoin block (host only).
    #[serde(rename = "boot")]
    Boot {
        /// Target user wire ID.
        #[serde(rename = "userId")]
        user_id: String,
    },
    /// Fill the open opponent seat with a clearly badged computer bot (host only).
    #[serde(rename = "bot.add")]
    BotAdd {
        /// Bot difficulty tier.
        level: BotLevelDto,
    },
    /// Remove the seated computer bot (host only).
    #[serde(rename = "bot.remove")]
    BotRemove {},
    /// Report another user for abuse; server attaches in-memory chat log.
    #[serde(rename = "report")]
    Report {
        /// Reported user wire ID.
        #[serde(rename = "userId")]
        user_id: String,
        /// Short abuse reason.
        reason: String,
    },
    /// Moderator action: mute a user for `minutes`.
    #[serde(rename = "mod.mute")]
    ModMute {
        /// Target user wire ID.
        #[serde(rename = "userId")]
        user_id: String,
        /// Duration in minutes.
        minutes: u32,
    },
    /// Moderator action: ban a user for `hours` (increments `session_version`).
    #[serde(rename = "mod.ban")]
    ModBan {
        /// Target user wire ID.
        #[serde(rename = "userId")]
        user_id: String,
        /// Duration in hours.
        hours: u32,
    },
    /// Moderator action: forcibly close an active table.
    #[serde(rename = "mod.close_table")]
    ModCloseTable {
        /// Table ID to close.
        #[serde(rename = "tableId")]
        table_id: String,
    },
}

/// Messages emitted from transport (`LocalSessionAdapter` or `boomer-server`) to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ServerMessage {
    /// Handshake acknowledgement.
    #[serde(rename = "welcome")]
    Welcome {
        /// Negotiated protocol version.
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        /// Session transport kind.
        mode: SessionKindDto,
        /// Authenticated or guest identity (`None` in Step 1 offline local mode).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        me: Option<UserDto>,
        /// Reconnect token issued to newly created guest sessions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guest: Option<String>,
        /// Available lounge rooms on the server.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rooms: Vec<RoomSummaryDto>,
    },
    /// Confirmation that a magic-link request was accepted (never reveals account existence).
    #[serde(rename = "auth.sent")]
    AuthSent {},
    /// Successful passwordless authentication response.
    #[serde(rename = "auth.ok")]
    AuthOk {
        /// Stateless signed session token stored in `localStorage`.
        session: String,
        /// Authenticated user summary.
        me: UserDto,
        /// `true` if this account was just created (prompt for name/avatar).
        #[serde(rename = "isNew")]
        is_new: bool,
    },
    /// Profile response for a registered player.
    #[serde(rename = "profile")]
    Profile {
        /// User summary.
        user: UserDto,
        /// Member-since UTC Unix timestamp in milliseconds.
        since: i64,
        /// Per-game rating & W/L/D stats.
        stats: Vec<ProfileGameStatDto>,
        /// Last 20 finished matches.
        last: Vec<ProfileMatchDto>,
    },
    /// Per-game High Score table response.
    #[serde(rename = "highscores")]
    Highscores {
        /// Game kind.
        game: GameKindDto,
        /// Ordered leaderboard rows.
        rows: Vec<HighscoreRowDto>,
    },
    /// Updated list of open lounge rooms (sent when rooms auto-open or counts change).
    #[serde(rename = "rooms.update")]
    RoomsUpdate {
        /// Current room summaries.
        rooms: Vec<RoomSummaryDto>,
    },
    /// Full room snapshot delivered upon `lobby.join`.
    #[serde(rename = "lobby.snapshot")]
    LobbySnapshot(Box<LobbySnapshotDto>),
    /// User joined the current lounge room.
    #[serde(rename = "user.add")]
    UserAdd(UserDto),
    /// User presence or rating changed in the current lounge room.
    #[serde(rename = "user.update")]
    UserUpdate(UserDto),
    /// User left the current lounge room.
    #[serde(rename = "user.remove")]
    UserRemove {
        /// Departed user wire ID.
        id: String,
    },
    /// Table created or updated in the current lounge room.
    #[serde(rename = "table.upsert")]
    TableUpsert(Box<TableSummaryDto>),
    /// Table removed from the current lounge room.
    #[serde(rename = "table.remove")]
    TableRemove {
        /// Closed table ID.
        id: String,
    },
    /// Incoming chat or system line for either lobby or table chat.
    #[serde(rename = "chat")]
    Chat {
        /// Whether this line belongs to the lobby room or active table.
        scope: ChatScopeDto,
        /// Chat line payload.
        #[serde(flatten)]
        line: ChatLineDto,
    },
    /// Direct 30-second invite received from another player.
    #[serde(rename = "invite.received")]
    InviteReceived {
        /// Unique invite identifier.
        #[serde(rename = "inviteId")]
        invite_id: String,
        /// Inviting player.
        from: UserDto,
        /// Game kind.
        game: GameKindDto,
        /// Optional existing table ID.
        #[serde(rename = "tableId", default, skip_serializing_if = "Option::is_none")]
        table_id: Option<String>,
        /// Proposed table settings.
        settings: TableSettingsDto,
        /// Remaining milliseconds before automatic expiry (30 000 ms initially).
        #[serde(rename = "msLeft")]
        ms_left: u64,
    },
    /// Pending invite declined, expired, or cancelled.
    #[serde(rename = "invite.closed")]
    InviteClosed {
        /// Invite identifier.
        #[serde(rename = "inviteId")]
        invite_id: String,
        /// Closure reason.
        reason: InviteCloseReasonDto,
    },
    /// Caller entered the human-first `Play Now` queue.
    #[serde(rename = "play.now.queued")]
    PlayNowQueued {
        /// Milliseconds before bot suggestion triggers (20 000 ms).
        #[serde(rename = "msWait")]
        ms_wait: u64,
    },
    /// `Play Now` waited 20 seconds without finding a human opponent; suggests playing a bot.
    #[serde(rename = "play.now.timeout")]
    PlayNowTimeout {
        /// Whether the UI should highlight a one-click computer opponent option.
        #[serde(rename = "suggestBot")]
        suggest_bot: bool,
    },
    /// Full table state snapshot.
    #[serde(rename = "table.snapshot")]
    TableSnapshot(Box<TableSnapshotDto>),
    /// Incremental table metadata update.
    #[serde(rename = "table.update")]
    TableUpdate(Box<TableUpdateDto>),
    /// Client was booted by the host or the table was closed.
    #[serde(rename = "kicked")]
    Kicked {
        /// Kick reason.
        reason: KickReasonDto,
    },
    /// Authoritative match-over broadcast with Elo deltas and High Score updates.
    #[serde(rename = "game.over")]
    GameOver(Box<GameOverDto>),
    /// Post-match rematch confirmation flags for Seat 1 and Seat 2.
    #[serde(rename = "rematch.state")]
    RematchState {
        /// Ready-for-rematch flags per seat.
        ready: [bool; 2],
    },
    /// Match paused while a disconnected seated player is within the 45-second grace period.
    #[serde(rename = "game.pause")]
    GamePause {
        /// Seat that disconnected.
        seat: SeatDto,
        /// Remaining grace period milliseconds (`45_000` initially).
        #[serde(rename = "msLeft")]
        ms_left: u64,
    },
    /// Match resumed after the disconnected seated player reconnected in time.
    #[serde(rename = "game.resume")]
    GameResume {},
    /// Confirmation that a moderation report was recorded.
    #[serde(rename = "report.saved")]
    ReportSaved {
        /// Persisted report ID.
        id: i64,
    },
    /// Current offline session summary.
    #[serde(rename = "session.state")]
    SessionState {
        /// Active game, or `None` when on the home screen.
        game: Option<GameKindDto>,
        /// Active mode, or `None` when on the home screen.
        mode: Option<LocalModeDto>,
        /// Human-readable status key.
        status: String,
    },
    /// Authoritative Checkers state snapshot.
    #[serde(rename = "checkers.snapshot")]
    CheckersSnapshot {
        /// Complete Checkers state.
        state: CheckersStateDto,
    },
    /// Authoritative Pool state snapshot (join, reconnect, placement, clock, or rematch).
    #[serde(rename = "pool.snapshot")]
    PoolSnapshot {
        /// Complete Pool state.
        state: PoolStateDto,
    },
    /// Authoritative Pool shot resolution: client replays `shot` from `pre_balls` via shared WASM physics
    /// and snaps cleanly to `state.balls` upon completion.
    #[serde(rename = "pool.shot")]
    PoolShot {
        /// Exact pre-shot resting balls (`0..=15`) used to start the deterministic client replay.
        #[serde(rename = "preBalls")]
        pre_balls: Vec<PoolBallDto>,
        /// Verified shot parameters.
        shot: PoolShotDto,
        /// Authoritative post-shot state revealed once balls come to rest.
        state: PoolStateDto,
    },
    /// Structured protocol error response.
    #[serde(rename = "error")]
    Error {
        /// Machine-readable error code.
        code: ErrorCode,
        /// Message type that triggered the error.
        re: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_round_trips_tagged_envelope() {
        let msg = ClientMessage::LocalStart {
            game: GameKindDto::Pool,
            mode: LocalModeDto::VsBot {
                level: BotLevelDto::Medium,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"t\":\"local.start\""));
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn checkers_move_round_trips_envelope() {
        let msg = ClientMessage::CheckersMove {
            turn: 4,
            path: vec![22, 15, 8],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"t\":\"checkers.move\""));
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn pool_shot_round_trips_envelope() {
        let msg = ClientMessage::PoolShoot {
            shot: PoolShotDto {
                turn: 1,
                angle: 0.25,
                power: 0.72,
                spin_x: -0.2,
                spin_y: 0.4,
                cue_pos: Some([0.635, 0.635]),
                called_pocket: Some(2),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"t\":\"pool.shoot\""));
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn server_error_round_trips_snake_case_code() {
        let msg = ServerMessage::Error {
            code: ErrorCode::NotYourTurn,
            re: "action".to_owned(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"not_your_turn\""));
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }
}

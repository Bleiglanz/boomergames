//! Versioned JSON message envelopes (`{ "t": "<type>", "d": { ... } }`).

use serde::{Deserialize, Serialize};

use crate::auth::{HighscoreRowDto, ProfileGameStatDto, ProfileMatchDto, RoomSummaryDto, UserDto};
use crate::checkers::{CheckersSettingsDto, CheckersStateDto};
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

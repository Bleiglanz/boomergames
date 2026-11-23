//! Versioned JSON message envelopes (`{ "t": "<type>", "d": { ... } }`).

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Messages emitted from transport (`LocalSessionAdapter` or `boomer-server`) to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    },
    /// Current offline session summary (Step 1 scaffold; extended with game states in Milestones 1 & 2).
    #[serde(rename = "session.state")]
    SessionState {
        /// Active game, or `None` when on the home screen.
        game: Option<GameKindDto>,
        /// Active mode, or `None` when on the home screen.
        mode: Option<LocalModeDto>,
        /// Human-readable status key.
        status: String,
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

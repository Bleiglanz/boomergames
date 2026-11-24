//! Acyclic actor runtime for Boomergames lounges and tables.
//!
//! Concurrency contract (Milestone 5.1):
//! - **Strict acyclic hierarchy:** [`LobbyHub`] → [`lobby_actor::LobbyActor`] → [`table_actor::TableActor`].
//!   `LobbyActor` and `TableActor` NEVER mutually `.await` each other on bounded channels.
//!   Table-to-lobby notifications (`LobbyEvent`) are strictly fire-and-forget (`try_send`).
//! - **Non-blocking per-client outbound queues:** Actors broadcast exclusively via `try_send`
//!   into each connection's bounded [`ClientSink`]. If a slow client's buffer overflows, its
//!   `slow_disconnect` flag is tripped and its socket disconnects cleanly without stalling any actor.

pub mod game_session;
pub mod hub;
pub mod lobby_actor;
pub mod table_actor;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::ws::Message;
use boomer_protocol::{BotLevelDto, ServerMessage, UserDto, UserStatusDto};
use tokio::sync::{Notify, mpsc};

/// Maximum chat line length in UTF-8 characters (`incubation/plan.md` §9.6).
pub const MAX_CHAT_CHARS: usize = 300;

/// Maximum retained chat lines per lobby room or table in memory.
pub const MAX_CHAT_HISTORY: usize = 50;

/// Chat profanity blocklist replaced with `***` (case-insensitive word/substring filter).
const CHAT_PROFANITY: &[&str] = &[
    "fuck", "shit", "bitch", "cunt", "nigger", "faggot", "hitler",
];

/// Non-blocking outbound handle for a single connected WebSocket client.
#[derive(Debug, Clone)]
pub struct ClientSink {
    /// Stable wire ID (`"u123"` or `"g1000"`).
    pub wire_id: String,
    out_tx: mpsc::Sender<Message>,
    slow_disconnect: Arc<AtomicBool>,
    disconnect_notify: Arc<Notify>,
}

impl ClientSink {
    /// Wraps a connection's bounded outbound channel with slow-client tripwire signaling.
    #[must_use]
    pub fn new(
        wire_id: String,
        out_tx: mpsc::Sender<Message>,
        slow_disconnect: Arc<AtomicBool>,
        disconnect_notify: Arc<Notify>,
    ) -> Self {
        Self {
            wire_id,
            out_tx,
            slow_disconnect,
            disconnect_notify,
        }
    }

    /// Sends a pre-serialized text frame via non-blocking `try_send`.
    /// Trips the slow-client disconnect signal if the outbound queue is full.
    pub fn send_frame(&self, frame: Message) -> bool {
        match self.out_tx.try_send(frame) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.slow_disconnect.store(true, Ordering::Release);
                self.disconnect_notify.notify_one();
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// Serializes and enqueues `msg` without blocking the calling actor.
    #[must_use]
    pub fn send_msg(&self, msg: &ServerMessage) -> bool {
        let Ok(json) = serde_json::to_string(msg) else {
            return false;
        };
        self.send_frame(Message::Text(json.into()))
    }

    /// Forces the underlying WebSocket connection to disconnect cleanly (used on moderator ban).
    pub fn force_disconnect(&self) {
        self.slow_disconnect.store(true, Ordering::Release);
        self.disconnect_notify.notify_one();
    }
}

/// Serializes `msg` once and broadcasts it via non-blocking `try_send` to every sink.
pub fn broadcast_msg<'a, I>(sinks: I, msg: &ServerMessage)
where
    I: IntoIterator<Item = &'a ClientSink>,
{
    let Ok(json) = serde_json::to_string(msg) else {
        return;
    };
    let frame = Message::Text(json.into());
    for sink in sinks {
        let _ = sink.send_frame(frame.clone());
    }
}

/// Sanitizes, trims, length-checks, and profanity-filters a chat message.
/// Returns `None` if empty or exceeding [`MAX_CHAT_CHARS`].
#[must_use]
pub fn sanitize_chat_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_CHAT_CHARS {
        return None;
    }
    let clean: String = trimmed
        .chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect();
    if clean.trim().is_empty() {
        return None;
    }

    let mut out = clean;
    for banned in CHAT_PROFANITY {
        out = censor_ascii_case_insensitive(&out, banned);
    }
    Some(out)
}

fn censor_ascii_case_insensitive(input: &str, needle: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if !lower.contains(needle) {
        return input.to_owned();
    }
    let mut result = String::with_capacity(input.len());
    let mut search_from = 0;
    while let Some(rel_idx) = lower
        .get(search_from..)
        .and_then(|slice| slice.find(needle))
    {
        let abs_idx = search_from + rel_idx;
        if let Some(prefix) = input.get(search_from..abs_idx) {
            result.push_str(prefix);
        }
        result.push_str("***");
        search_from = abs_idx + needle.len();
    }
    if let Some(tail) = input.get(search_from..) {
        result.push_str(tail);
    }
    result
}

/// Builds a clearly badged [`UserDto`] representing a computer opponent.
#[must_use]
pub fn bot_user_dto(level: BotLevelDto, status: UserStatusDto) -> UserDto {
    let (id, name, rating) = match level {
        BotLevelDto::Easy => ("bot1", "House Bot (Easy)", 1000),
        BotLevelDto::Medium => ("bot2", "House Bot (Medium)", 1250),
        BotLevelDto::Hard => ("bot3", "House Bot (Hard)", 1500),
    };
    UserDto {
        id: id.to_owned(),
        name: name.to_owned(),
        avatar: 0,
        rating,
        prov: false,
        status,
        bot: true,
        guest: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_sanitizer_enforces_length_and_profanity_filter() {
        assert!(sanitize_chat_text("   ").is_none());
        let too_long = "a".repeat(MAX_CHAT_CHARS + 1);
        assert!(sanitize_chat_text(&too_long).is_none());
        assert_eq!(
            sanitize_chat_text("What the FUCK, nice shot!").as_deref(),
            Some("What the ***, nice shot!")
        );
    }
}

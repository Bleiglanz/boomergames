//! Per-chatroom state machine driving the optional Claude AI Friend companion.
//!
//! Each [`crate::actors::lobby_actor::LobbyActor`] and [`crate::actors::table_actor::TableActor`]
//! owns an independent [`AiChatState`], ensuring lounge rooms and individual tables track their
//! own On/Off toggle, message cadence (2–3 human messages), cooldown, and in-flight cancellation.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use boomer_protocol::{ChatLineDto, GameKindDto};
use chrono::Utc;

use crate::actors::{MAX_CHAT_CHARS, sanitize_chat_text};
use crate::ports::AiChatTurn;

/// Wire sender ID assigned to AI Friend chat lines.
pub const AI_FRIEND_WIRE_ID: &str = "ai_claude";

/// Display name shown in chat for AI Friend replies.
pub const AI_FRIEND_DISPLAY_NAME: &str = "🤖 Claude (AI)";

/// Maximum number of recent non-system chat lines sent as context to Claude.
pub const AI_CONTEXT_HISTORY_LIMIT: usize = 8;

/// Default minimum delay between consecutive AI replies in the same chatroom.
pub const DEFAULT_AI_REPLY_COOLDOWN: Duration = Duration::from_secs(8);

/// Minimum delay between consecutive AI replies when chatting solo with a single human.
pub const SOLO_AI_REPLY_COOLDOWN: Duration = Duration::from_secs(2);

/// Independent per-chatroom AI Friend controller (zero shared locks).
#[derive(Debug)]
pub struct AiChatState {
    enabled: bool,
    human_msgs_since_reply: u8,
    next_threshold: u8,
    in_flight: bool,
    generation: u64,
    last_reply_at: Option<Instant>,
}

impl AiChatState {
    /// Creates a new per-chatroom state machine with initial `enabled` toggle.
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            human_msgs_since_reply: 0,
            next_threshold: 2,
            in_flight: false,
            generation: 0,
            last_reply_at: None,
        }
    }

    /// Returns whether AI Friend is currently enabled in this chatroom.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Updates the enabled toggle and invalidates any in-flight request if disabled.
    /// Returns `true` if the state actually changed.
    pub const fn set_enabled(&mut self, enabled: bool) -> bool {
        if self.enabled == enabled {
            return false;
        }
        self.enabled = enabled;
        self.human_msgs_since_reply = 0;
        self.next_threshold = 2;
        self.in_flight = false;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Resets the chatroom back to a fresh state (e.g., when a House Bot table empties).
    pub const fn reset(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.human_msgs_since_reply = 0;
        self.next_threshold = 2;
        self.in_flight = false;
        self.generation = self.generation.wrapping_add(1);
        self.last_reply_at = None;
    }

    /// Records a new human chat message and returns `Some(generation)` when Claude should reply.
    ///
    /// When `solo_human` is `true` (only one human in the chatroom), replies immediately on every
    /// message (with a gentle solo cooldown). With 2+ humans, chimes in every 2–3 messages.
    pub fn on_human_message(
        &mut self,
        now: Instant,
        cooldown: Duration,
        solo_human: bool,
    ) -> Option<u64> {
        if !self.enabled || self.in_flight {
            return None;
        }
        self.human_msgs_since_reply = self.human_msgs_since_reply.saturating_add(1);
        let threshold = if solo_human { 1 } else { self.next_threshold };
        if self.human_msgs_since_reply < threshold {
            return None;
        }
        let effective_cooldown = if solo_human {
            cooldown.min(SOLO_AI_REPLY_COOLDOWN)
        } else {
            cooldown
        };
        if let Some(last) = self.last_reply_at
            && now.duration_since(last) < effective_cooldown
        {
            return None;
        }
        self.in_flight = true;
        self.human_msgs_since_reply = 0;
        if !solo_human {
            // Alternate cleanly between 2 and 3 messages so group replies feel organic.
            self.next_threshold = if self.next_threshold == 2 { 3 } else { 2 };
        }
        Some(self.generation)
    }

    /// Completes an in-flight AI request. Returns `true` if the reply is still current and
    /// should be displayed in chat, or `false` if it was invalidated by a toggle/reset.
    pub fn finish_request(&mut self, generation: u64, succeeded: bool, now: Instant) -> bool {
        if generation != self.generation {
            return false;
        }
        self.in_flight = false;
        if !self.enabled {
            return false;
        }
        if succeeded {
            self.last_reply_at = Some(now);
        }
        true
    }
}

/// Builds a compact recent transcript (last [`AI_CONTEXT_HISTORY_LIMIT`] non-system lines).
#[must_use]
pub fn recent_chat_turns(chat: &VecDeque<ChatLineDto>) -> Vec<AiChatTurn> {
    let non_sys: Vec<&ChatLineDto> = chat.iter().filter(|line| !line.sys).collect();
    let start = non_sys.len().saturating_sub(AI_CONTEXT_HISTORY_LIMIT);
    non_sys
        .get(start..)
        .unwrap_or(&[])
        .iter()
        .map(|line| AiChatTurn {
            speaker: line.name.clone().unwrap_or_else(|| "Player".to_owned()),
            text: line.text.clone(),
            is_ai: line.ai,
        })
        .collect()
}

/// Safely truncates and sanitizes an AI reply to fit within [`MAX_CHAT_CHARS`].
#[must_use]
pub fn sanitize_ai_reply(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clipped = if trimmed.chars().count() <= MAX_CHAT_CHARS {
        trimmed.to_owned()
    } else {
        let mut buf: String = trimmed.chars().take(MAX_CHAT_CHARS - 1).collect();
        if let Some(last_space) = buf.rfind(' ')
            && last_space > MAX_CHAT_CHARS / 2
        {
            buf.truncate(last_space);
        }
        buf.push('…');
        buf
    };
    sanitize_chat_text(&clipped)
}

/// Builds a broadcast-ready [`ChatLineDto`] authored by Claude (AI Friend).
#[must_use]
pub fn make_ai_chat_line(text: String) -> ChatLineDto {
    ChatLineDto {
        from: Some(AI_FRIEND_WIRE_ID.to_owned()),
        name: Some(AI_FRIEND_DISPLAY_NAME.to_owned()),
        text,
        ts: Utc::now().timestamp_millis(),
        sys: false,
        ai: true,
    }
}

/// Human-friendly game name used in Claude's one-line context header.
#[must_use]
pub const fn game_context_name(game: GameKindDto) -> &'static str {
    match game {
        GameKindDto::Pool => "8-Ball Pool",
        GameKindDto::Checkers => "Checkers",
        GameKindDto::Chess => "Chess",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_after_two_then_three_messages_and_respects_cooldown() {
        let mut state = AiChatState::new(true);
        let t0 = Instant::now();
        let cd = Duration::from_secs(8);

        assert_eq!(state.on_human_message(t0, cd, false), None);
        let gen1 = state
            .on_human_message(t0, cd, false)
            .expect("2nd message triggers");
        // Overlapping messages while in-flight do not trigger another call.
        assert_eq!(state.on_human_message(t0, cd, false), None);
        assert!(state.finish_request(gen1, true, t0));

        // Next threshold is 3 messages, and cooldown blocks early replies.
        assert_eq!(
            state.on_human_message(t0 + Duration::from_secs(1), cd, false),
            None
        );
        assert_eq!(
            state.on_human_message(t0 + Duration::from_secs(2), cd, false),
            None
        );
        // 3rd message still within 8s cooldown -> waits until cooldown passes.
        assert_eq!(
            state.on_human_message(t0 + Duration::from_secs(3), cd, false),
            None
        );
        let gen2 = state
            .on_human_message(t0 + Duration::from_secs(9), cd, false)
            .expect("triggers once cooldown elapses");
        assert!(state.finish_request(gen2, true, t0 + Duration::from_secs(9)));
    }

    #[test]
    fn solo_player_triggers_immediately_on_first_message() {
        let mut state = AiChatState::new(true);
        let t0 = Instant::now();
        let cd = Duration::from_secs(8);

        let gen1 = state
            .on_human_message(t0, cd, true)
            .expect("solo 1st message triggers immediately");
        assert!(state.finish_request(gen1, true, t0));

        // Solo cooldown is capped at 2s (shorter than 8s group cooldown).
        assert_eq!(
            state.on_human_message(t0 + Duration::from_secs(1), cd, true),
            None
        );
        let gen2 = state
            .on_human_message(t0 + Duration::from_secs(3), cd, true)
            .expect("solo follow-up triggers after 2s cooldown");
        assert!(state.finish_request(gen2, true, t0 + Duration::from_secs(3)));
    }

    #[test]
    fn toggle_off_cancels_in_flight_reply() {
        let mut state = AiChatState::new(true);
        let t0 = Instant::now();
        let cd = Duration::ZERO;

        assert_eq!(state.on_human_message(t0, cd, false), None);
        let generation = state.on_human_message(t0, cd, false).expect("triggers");
        assert!(state.set_enabled(false));
        assert!(!state.finish_request(generation, true, t0));
    }

    #[test]
    fn sanitize_ai_reply_truncates_long_text_cleanly() {
        let long = "hello ".repeat(80);
        let clean = sanitize_ai_reply(&long).expect("non-empty");
        assert!(clean.chars().count() <= MAX_CHAT_CHARS);
        assert!(clean.ends_with('…'));
    }
}

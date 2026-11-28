//! Anthropic Claude Messages API adapter for the optional AI Friend chat companion.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::config::AnthropicConfig;
use crate::ports::{AiChatContext, AiFriend, BoxFuture, PortError};

/// Anthropic Messages API endpoint.
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Required Anthropic API version header.
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// HTTP timeout guarding against slow upstream responses.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// Maximum concurrent in-flight requests to Anthropic across all chatrooms.
const MAX_CONCURRENT_CALLS: usize = 2;

/// Concise system prompt establishing Claude's warm, dad-joke-loving observer persona
/// plus a compact Boomergames UI cheat sheet for helping players.
pub const AI_FRIEND_SYSTEM_PROMPT: &str = "\
You are Claude, a warm and cheerful observer hanging out in a classic online game lounge \
(8-Ball Pool, Checkers, or Chess) on Boomergames. You chime in casually as a friendly AI \
companion (or chat directly if only one player is here). Be very nice and welcoming to people, \
tell an occasional cheesy bad dad joke (bonus points for Pool, Checkers, or Chess puns when \
they fit naturally), and always try to lift the players' spirits. Never be mean, snarky, \
sarcastic, or preachy. Reply in the same language the players are using (e.g., English or German). \
If someone asks how the site works, help them concisely using these real UI controls: \
switch games via the Room dropdown in the lounge toolbar (or click Offline Play in the header for solo/Stockfish Chess); \
click Play Now for quick matchmaking, Create Table to host your own table, or Join/Watch any table (including always-open House Bot tables), then press Ready/Start; \
click Copy Link at a table to invite a friend; \
click your name/Sign In (Google or Email Magic Link, zero cookies) for rated Elo, High Scores, and Profiles; \
header has Sound/Volume, Jazz music, Themes (including High Contrast), and EN/DE language; \
in-game: Pool uses drag-back slingshot aiming (drag cue ball for ball-in-hand), Checkers/Chess use click or drag, and pressing F opens Full-Screen (right-drag/Shift-drag/scroll or 2-finger pinch tilts into a secret 3D table/board view; C = behind-cue cam in Pool; R = reset to 2D). \
Keep every reply to 1-2 short conversational sentences under 240 characters, plain text only, \
with no markdown formatting and no speaker prefix like \"Claude:\".";

#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: [AnthropicMessage; 1],
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

/// Live Anthropic Messages API adapter with hard call-budget & concurrency guardrails.
pub struct AnthropicAiFriend {
    api_key: String,
    model: String,
    max_tokens: u32,
    max_calls: u32,
    system_prompt_file: PathBuf,
    calls_used: AtomicU32,
    concurrency: Arc<Semaphore>,
    http: reqwest::Client,
}

/// Reads and trims an optional UTF-8 system prompt override file from disk.
///
/// Returns `Ok(None)` if the file does not exist or contains only whitespace.
///
/// # Errors
/// Returns an [`std::io::Error`] if the file exists but cannot be read.
pub fn read_system_prompt_from_path(path: &Path) -> Result<Option<String>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

impl AnthropicAiFriend {
    /// Creates a new [`AnthropicAiFriend`] from [`AnthropicConfig`].
    #[must_use]
    pub fn new(config: &AnthropicConfig) -> Self {
        match read_system_prompt_from_path(&config.system_prompt_file) {
            Ok(Some(prompt)) => {
                info!(
                    path = %config.system_prompt_file.display(),
                    chars = prompt.chars().count(),
                    "loaded custom AI Friend system prompt file"
                );
            }
            Ok(None) if config.system_prompt_file_explicit => {
                warn!(
                    path = %config.system_prompt_file.display(),
                    "ANTHROPIC_SYSTEM_PROMPT_FILE not found or empty; using built-in AI Friend system prompt"
                );
            }
            Ok(None) => {
                info!(
                    path = %config.system_prompt_file.display(),
                    "custom AI Friend system prompt file not present; using built-in prompt"
                );
            }
            Err(err) => {
                warn!(
                    path = %config.system_prompt_file.display(),
                    %err,
                    "failed to read AI Friend system prompt file; using built-in prompt"
                );
            }
        }
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            max_calls: config.max_calls,
            system_prompt_file: config.system_prompt_file.clone(),
            calls_used: AtomicU32::new(0),
            concurrency: Arc::new(Semaphore::new(MAX_CONCURRENT_CALLS)),
            http,
        }
    }

    /// Loads the configured system prompt file on demand (allowing live edits without restart),
    /// falling back cleanly to [`AI_FRIEND_SYSTEM_PROMPT`] if absent, empty, or unreadable.
    async fn resolve_system_prompt(&self) -> String {
        match tokio::fs::read_to_string(&self.system_prompt_file).await {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    AI_FRIEND_SYSTEM_PROMPT.to_owned()
                } else {
                    trimmed.to_owned()
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                AI_FRIEND_SYSTEM_PROMPT.to_owned()
            }
            Err(err) => {
                warn!(
                    path = %self.system_prompt_file.display(),
                    %err,
                    "failed to read AI Friend system prompt file; falling back to built-in prompt"
                );
                AI_FRIEND_SYSTEM_PROMPT.to_owned()
            }
        }
    }

    /// Formats compact context and recent chat lines into a single user prompt.
    #[must_use]
    pub fn format_user_prompt(ctx: &AiChatContext) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("Context: ");
        out.push_str(&ctx.setting);
        out.push_str("\nRecent chat:\n");
        for turn in &ctx.recent_messages {
            out.push_str(&turn.speaker);
            out.push_str(": ");
            out.push_str(&turn.text);
            out.push('\n');
        }
        out.push_str(
            "\nChime in naturally with 1-2 short friendly sentences (plain text, under 240 chars).",
        );
        out
    }

    /// Attempts to reserve one call from the hard server-wide budget.
    fn try_reserve_call(&self) -> Result<u32, PortError> {
        let mut cur = self.calls_used.load(Ordering::Acquire);
        loop {
            if cur >= self.max_calls {
                return Err(PortError::Conflict("ai_budget_exhausted".to_owned()));
            }
            match self.calls_used.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(cur + 1),
                Err(actual) => cur = actual,
            }
        }
    }

    /// Extracts and cleans plain text from a raw Anthropic Messages API JSON response.
    fn parse_reply_body(body: &str) -> Result<(String, Option<AnthropicUsage>), PortError> {
        let decoded: AnthropicResponse = serde_json::from_str(body)
            .map_err(|err| PortError::Unavailable(format!("anthropic decode failed: {err}")))?;
        let combined = decoded
            .content
            .into_iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join(" ");
        let cleaned = clean_model_reply(&combined);
        if cleaned.is_empty() {
            return Err(PortError::Unavailable(
                "anthropic returned empty text".to_owned(),
            ));
        }
        Ok((cleaned, decoded.usage))
    }
}

/// Cleans whitespace, newlines, and accidental speaker prefixes from model output.
#[must_use]
pub fn clean_model_reply(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim().trim_matches('"').trim();
    let without_prefix = ["🤖 Claude (AI):", "Claude (AI):", "Claude:", "AI Friend:"]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix).map(str::trim))
        .unwrap_or(trimmed);
    without_prefix.trim_matches('"').trim().to_owned()
}

impl AiFriend for AnthropicAiFriend {
    fn is_configured(&self) -> bool {
        true
    }

    fn generate_reply(&self, ctx: AiChatContext) -> BoxFuture<'_, Result<String, PortError>> {
        Box::pin(async move {
            let call_no = self.try_reserve_call()?;
            let Ok(_permit) = self.concurrency.acquire().await else {
                return Err(PortError::Unavailable("ai semaphore closed".to_owned()));
            };

            let system_prompt = self.resolve_system_prompt().await;
            let req_body = AnthropicRequest {
                model: &self.model,
                max_tokens: self.max_tokens,
                system: &system_prompt,
                messages: [AnthropicMessage {
                    role: "user",
                    content: Self::format_user_prompt(&ctx),
                }],
            };

            let resp = self
                .http
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_API_VERSION)
                .json(&req_body)
                .send()
                .await
                .map_err(|err| {
                    warn!(%err, "Anthropic API request failed");
                    PortError::Unavailable("anthropic request failed".to_owned())
                })?;

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                warn!(status = %status, "Anthropic API returned non-success status");
                return Err(PortError::Unavailable(format!(
                    "anthropic status {}",
                    status.as_u16()
                )));
            }

            let (reply, usage) = Self::parse_reply_body(&body)?;
            let (in_tok, out_tok) = usage.map_or((0, 0), |u| (u.input_tokens, u.output_tokens));
            info!(
                model = %self.model,
                input_tokens = in_tok,
                output_tokens = out_tok,
                calls_used = call_no,
                max_calls = self.max_calls,
                "dispatched Anthropic AI Friend reply"
            );
            Ok(reply)
        })
    }
}

/// No-op adapter used when `ANTHROPIC_API_KEY` is unset (and in automated tests).
#[derive(Debug, Default)]
pub struct DisabledAiFriend;

impl AiFriend for DisabledAiFriend {
    fn is_configured(&self) -> bool {
        false
    }

    fn generate_reply(&self, _ctx: AiChatContext) -> BoxFuture<'_, Result<String, PortError>> {
        Box::pin(async move {
            Err(PortError::Unavailable(
                "AI Friend is not configured on this server".to_owned(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::AiChatTurn;

    #[test]
    fn clean_model_reply_strips_prefix_and_collapses_whitespace() {
        assert_eq!(
            clean_model_reply("  Claude: \"Nice shot!\nKeep it rolling!\" "),
            "Nice shot! Keep it rolling!"
        );
        assert_eq!(
            clean_model_reply("🤖 Claude (AI): Why did the knight quit? Too many L's!"),
            "Why did the knight quit? Too many L's!"
        );
    }

    #[test]
    fn parse_reply_body_extracts_text_and_usage() {
        let json = r#"{
            "id": "msg_01",
            "type": "message",
            "content": [{"type": "text", "text": "Great vibes in here!"}],
            "usage": {"input_tokens": 210, "output_tokens": 12}
        }"#;
        let (text, usage) = AnthropicAiFriend::parse_reply_body(json).unwrap();
        assert_eq!(text, "Great vibes in here!");
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 210);
        assert_eq!(usage.output_tokens, 12);
    }

    #[test]
    fn call_budget_cap_blocks_excess_calls() {
        let cfg = AnthropicConfig {
            api_key: "test-key-never-sent".to_owned(),
            model: "claude-haiku-4-5".to_owned(),
            max_tokens: 100,
            max_calls: 2,
            system_prompt_file: PathBuf::from("/nonexistent/boomer_ai_prompt_test.txt"),
            system_prompt_file_explicit: false,
        };
        let client = AnthropicAiFriend::new(&cfg);
        assert_eq!(client.try_reserve_call(), Ok(1));
        assert_eq!(client.try_reserve_call(), Ok(2));
        assert!(matches!(
            client.try_reserve_call(),
            Err(PortError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn system_prompt_file_overrides_default_and_reloads_live() {
        let path = std::env::temp_dir().join(format!(
            "boomer-ai-prompt-test-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let cfg = AnthropicConfig {
            api_key: "test-key-never-sent".to_owned(),
            model: "claude-haiku-4-5".to_owned(),
            max_tokens: 100,
            max_calls: 2,
            system_prompt_file: path.clone(),
            system_prompt_file_explicit: true,
        };
        let client = AnthropicAiFriend::new(&cfg);

        // Missing file -> falls back to built-in default.
        assert_eq!(
            client.resolve_system_prompt().await,
            AI_FRIEND_SYSTEM_PROMPT
        );

        // Custom file -> loaded and trimmed.
        std::fs::write(&path, "  Custom billiard host prompt.  \n").unwrap();
        assert_eq!(
            client.resolve_system_prompt().await,
            "Custom billiard host prompt."
        );

        // Live edit -> reloaded without recreating client.
        std::fs::write(&path, "Updated chess club prompt.").unwrap();
        assert_eq!(
            client.resolve_system_prompt().await,
            "Updated chess club prompt."
        );

        // Whitespace-only file -> falls back to built-in default.
        std::fs::write(&path, "   \n\t  ").unwrap();
        assert_eq!(
            client.resolve_system_prompt().await,
            AI_FRIEND_SYSTEM_PROMPT
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn format_user_prompt_includes_context_and_recent_lines() {
        let ctx = AiChatContext {
            setting: "Table #2 (Chess, playing: Alice vs Bob)".to_owned(),
            recent_messages: vec![
                AiChatTurn {
                    speaker: "Alice".to_owned(),
                    text: "Good luck!".to_owned(),
                    is_ai: false,
                },
                AiChatTurn {
                    speaker: "Bob".to_owned(),
                    text: "You too!".to_owned(),
                    is_ai: false,
                },
            ],
        };
        let prompt = AnthropicAiFriend::format_user_prompt(&ctx);
        assert!(prompt.contains("Table #2 (Chess"));
        assert!(prompt.contains("Alice: Good luck!"));
        assert!(prompt.contains("Bob: You too!"));
    }
}

//! Stateless HMAC-SHA256 token signing/verification and display-name/email validation.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Magic-link token lifetime (15 minutes per `incubation/plan.md` §9.5).
pub const MAGIC_TOKEN_TTL_SECS: i64 = 15 * 60;

/// Registered session token lifetime (30 days).
pub const SESSION_TOKEN_TTL_SECS: i64 = 30 * 24 * 60 * 60;

/// Guest reconnect token lifetime (24 hours).
pub const GUEST_TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

/// Display name cooldown between renames (30 days per `incubation/plan.md` A-3).
pub const NAME_CHANGE_COOLDOWN_DAYS: i64 = 30;

/// Maximum valid built-in avatar index (`0..16`).
pub const MAX_AVATAR_INDEX: i16 = 15;

/// Minimal baseline profanity blocklist (extended cleanly per room in Milestone 5).
const BLOCKED_SUBSTRINGS: &[&str] = &["admin", "moderator", "fuck", "shit", "nigger", "hitler"];

/// Signed session payload stored client-side in `localStorage` (no cookies, no DB sessions table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClaims {
    /// Registered `users.id`.
    pub user_id: i64,
    /// Must match `users.session_version` (incremented on ban / global logout).
    pub session_version: i32,
    /// Expiration Unix timestamp (seconds).
    pub exp: i64,
}

/// Signed magic-link payload carried in `/#login=<token>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicClaims {
    /// Normalized lowercase email address.
    pub email: String,
    /// Expiration Unix timestamp (seconds).
    pub exp: i64,
}

/// Signed guest reconnect payload allowing guests to preserve `Guest_XXXX` across reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestClaims {
    /// Numeric guest identifier (rendered as `Guest_<num>` / `"g<num>"`).
    pub guest_num: u32,
    /// Expiration Unix timestamp (seconds).
    pub exp: i64,
}

/// Errors produced when verifying a signed token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    /// Malformed structure or base64/JSON decoding failure.
    Malformed,
    /// HMAC signature did not match.
    InvalidSignature,
    /// Token expiration timestamp has passed.
    Expired,
}

/// Stateless HMAC-SHA256 signer for session, magic-link, and guest tokens.
#[derive(Debug, Clone)]
pub struct TokenSigner {
    secret: Vec<u8>,
}

impl TokenSigner {
    /// Creates a new signer from raw secret bytes.
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
        }
    }

    /// Signs a session token for `(user_id, session_version)`.
    ///
    /// # Errors
    /// Returns [`TokenError::Malformed`] if JSON encoding unexpectedly fails.
    pub fn sign_session(&self, user_id: i64, session_version: i32) -> Result<String, TokenError> {
        let claims = SessionClaims {
            user_id,
            session_version,
            exp: (Utc::now() + Duration::seconds(SESSION_TOKEN_TTL_SECS)).timestamp(),
        };
        self.sign_typed("s", &claims)
    }

    /// Verifies and decodes a session token.
    ///
    /// # Errors
    /// Returns [`TokenError`] if signature is invalid, malformed, or expired.
    pub fn verify_session(&self, token: &str) -> Result<SessionClaims, TokenError> {
        let claims: SessionClaims = self.verify_typed("s", token)?;
        if claims.exp < Utc::now().timestamp() {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    /// Signs a 15-minute magic-link login token for `email`.
    ///
    /// # Errors
    /// Returns [`TokenError::Malformed`] if JSON encoding unexpectedly fails.
    pub fn sign_magic(&self, email: &str) -> Result<String, TokenError> {
        let claims = MagicClaims {
            email: email.to_ascii_lowercase(),
            exp: (Utc::now() + Duration::seconds(MAGIC_TOKEN_TTL_SECS)).timestamp(),
        };
        self.sign_typed("m", &claims)
    }

    /// Verifies and decodes a magic-link token.
    ///
    /// # Errors
    /// Returns [`TokenError`] if signature is invalid, malformed, or expired.
    pub fn verify_magic(&self, token: &str) -> Result<MagicClaims, TokenError> {
        let claims: MagicClaims = self.verify_typed("m", token)?;
        if claims.exp < Utc::now().timestamp() {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    /// Signs a 24-hour guest reconnect token for `guest_num`.
    ///
    /// # Errors
    /// Returns [`TokenError::Malformed`] if JSON encoding unexpectedly fails.
    pub fn sign_guest(&self, guest_num: u32) -> Result<String, TokenError> {
        let claims = GuestClaims {
            guest_num,
            exp: (Utc::now() + Duration::seconds(GUEST_TOKEN_TTL_SECS)).timestamp(),
        };
        self.sign_typed("g", &claims)
    }

    /// Verifies and decodes a guest reconnect token.
    ///
    /// # Errors
    /// Returns [`TokenError`] if signature is invalid, malformed, or expired.
    pub fn verify_guest(&self, token: &str) -> Result<GuestClaims, TokenError> {
        let claims: GuestClaims = self.verify_typed("g", token)?;
        if claims.exp < Utc::now().timestamp() {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    fn sign_typed<T: Serialize>(&self, kind: &str, payload: &T) -> Result<String, TokenError> {
        let json = serde_json::to_vec(payload).map_err(|_| TokenError::Malformed)?;
        let encoded_payload = URL_SAFE_NO_PAD.encode(json);
        let signing_input = format!("{kind}.{encoded_payload}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| TokenError::Malformed)?;
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signing_input}.{sig}"))
    }

    fn verify_typed<T: for<'de> Deserialize<'de>>(
        &self,
        expected_kind: &str,
        token: &str,
    ) -> Result<T, TokenError> {
        let mut parts = token.split('.');
        let (Some(kind), Some(payload_b64), Some(sig_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(TokenError::Malformed);
        };
        if kind != expected_kind {
            return Err(TokenError::Malformed);
        }
        let sig_bytes = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| TokenError::Malformed)?;
        let signing_input = format!("{kind}.{payload_b64}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| TokenError::Malformed)?;
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&sig_bytes)
            .map_err(|_| TokenError::InvalidSignature)?;

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| TokenError::Malformed)?;
        serde_json::from_slice(&payload_bytes).map_err(|_| TokenError::Malformed)
    }
}

/// Validates and normalizes an email address.
#[must_use]
pub fn validate_email(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() < 5 || trimmed.len() > 254 || trimmed.contains(char::is_whitespace) {
        return None;
    }
    let (local, domain) = trimmed.split_once('@')?;
    if local.is_empty() || domain.len() < 3 || !domain.contains('.') || domain.contains('@') {
        return None;
    }
    Some(trimmed)
}

/// Validates a display name (`3..=20` chars, ASCII letters/digits/`_`/`-`, profanity filtered).
///
/// # Errors
/// Returns a static error reason if invalid.
pub fn validate_display_name(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if !(3..=20).contains(&trimmed.len()) {
        return Err("display name must be 3 to 20 characters");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("display name may only contain letters, digits, '_' and '-'");
    }
    if trimmed.to_ascii_lowercase().starts_with("guest_") {
        return Err("reserved guest prefix");
    }
    let lower = trimmed.to_ascii_lowercase();
    if BLOCKED_SUBSTRINGS.iter().any(|word| lower.contains(word)) {
        return Err("display name contains a disallowed term");
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_tokens_round_trip_and_reject_tampering() {
        let signer = TokenSigner::new(b"test-secret-key");
        let sess = signer.sign_session(42, 3).unwrap();
        let claims = signer.verify_session(&sess).unwrap();
        assert_eq!(claims.user_id, 42);
        assert_eq!(claims.session_version, 3);

        // Wrong kind rejected
        assert_eq!(signer.verify_magic(&sess), Err(TokenError::Malformed));

        // Tampered token rejected
        let tampered = format!("{sess}x");
        assert!(signer.verify_session(&tampered).is_err());

        // Magic & guest round-trip
        let magic = signer.sign_magic("Player@Example.COM").unwrap();
        assert_eq!(
            signer.verify_magic(&magic).unwrap().email,
            "player@example.com"
        );
        let guest = signer.sign_guest(4821).unwrap();
        assert_eq!(signer.verify_guest(&guest).unwrap().guest_num, 4821);
    }

    #[test]
    fn display_name_and_email_validation_enforces_rules() {
        assert_eq!(
            validate_email("  Alice@Boomer.DE "),
            Some("alice@boomer.de".to_owned())
        );
        assert!(validate_email("not-an-email").is_none());
        assert!(validate_display_name("LordOfTheRails").is_ok());
        assert!(validate_display_name("ab").is_err());
        assert!(validate_display_name("Guest_1234").is_err());
        assert!(validate_display_name("Bad!Name").is_err());
    }
}

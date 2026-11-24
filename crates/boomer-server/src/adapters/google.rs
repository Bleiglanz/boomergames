//! Google Identity Services ID token verification adapter.

use serde::Deserialize;

use crate::ports::{BoxFuture, GoogleTokenVerifier, PortError, VerifiedGoogleUser};

#[derive(Debug, Deserialize)]
struct GoogleTokenInfo {
    aud: String,
    sub: String,
    email: String,
    email_verified: Option<String>,
    name: Option<String>,
}

/// Verifies Google Identity Services ID tokens against Google's `tokeninfo` endpoint.
pub struct GoogleIdTokenVerifier {
    client_id: String,
    http: reqwest::Client,
}

impl GoogleIdTokenVerifier {
    /// Creates a new verifier bound to `client_id`.
    #[must_use]
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            http: reqwest::Client::new(),
        }
    }
}

impl GoogleTokenVerifier for GoogleIdTokenVerifier {
    fn verify_id_token(
        &self,
        id_token: &str,
    ) -> BoxFuture<'_, Result<VerifiedGoogleUser, PortError>> {
        let token = id_token.to_owned();
        Box::pin(async move {
            if token.trim().is_empty() {
                return Err(PortError::Invalid("empty Google ID token".to_owned()));
            }
            let resp = self
                .http
                .get("https://oauth2.googleapis.com/tokeninfo")
                .query(&[("id_token", token.as_str())])
                .send()
                .await
                .map_err(|err| PortError::Unavailable(format!("google verify failed: {err}")))?;

            if !resp.status().is_success() {
                return Err(PortError::Invalid("invalid Google ID token".to_owned()));
            }

            let info: GoogleTokenInfo = resp
                .json()
                .await
                .map_err(|err| PortError::Unavailable(format!("google decode failed: {err}")))?;

            if info.aud != self.client_id {
                return Err(PortError::Invalid(
                    "Google token audience mismatch".to_owned(),
                ));
            }
            if info.email_verified.as_deref() != Some("true") {
                return Err(PortError::Invalid(
                    "Google email is not verified".to_owned(),
                ));
            }

            Ok(VerifiedGoogleUser {
                sub: info.sub,
                email: info.email.to_ascii_lowercase(),
                suggested_name: info.name,
            })
        })
    }
}

/// Fallback adapter used when `GOOGLE_CLIENT_ID` is not configured.
#[derive(Debug, Default)]
pub struct UnconfiguredGoogleVerifier;

impl GoogleTokenVerifier for UnconfiguredGoogleVerifier {
    fn verify_id_token(
        &self,
        _id_token: &str,
    ) -> BoxFuture<'_, Result<VerifiedGoogleUser, PortError>> {
        Box::pin(async move {
            Err(PortError::Unavailable(
                "Google sign-in is not configured on this server".to_owned(),
            ))
        })
    }
}

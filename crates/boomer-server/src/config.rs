//! Environment-driven server configuration with safe local development defaults.

/// SMTP configuration for sending passwordless magic links (port 587 STARTTLS).
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// SMTP hostname (e.g., `smtp-relay.brevo.com`).
    pub host: String,
    /// SMTP port (typically `587`).
    pub port: u16,
    /// SMTP username.
    pub username: String,
    /// SMTP password.
    pub password: String,
    /// Sender address (e.g., `Boomergames <login@boomergames.example>`).
    pub from: String,
}

/// Default Anthropic Claude model used for the optional chat AI Friend.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-haiku-4-5";

/// Default maximum output tokens per AI Friend chat reply.
pub const DEFAULT_ANTHROPIC_MAX_TOKENS: u32 = 100;

/// Default server-wide safety cap on Anthropic API calls per process run.
pub const DEFAULT_ANTHROPIC_MAX_CALLS: u32 = 50;

/// Anthropic Messages API configuration for the optional Claude AI Friend.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// Secret Anthropic API key (`ANTHROPIC_API_KEY`).
    pub api_key: String,
    /// Model ID (`ANTHROPIC_MODEL`, default `claude-haiku-4-5`).
    pub model: String,
    /// Max output tokens per reply (`ANTHROPIC_MAX_TOKENS`, default `100`).
    pub max_tokens: u32,
    /// Hard server-wide call cap (`ANTHROPIC_MAX_CALLS`, default `50`).
    pub max_calls: u32,
}

/// Runtime configuration for `boomer-server`.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Socket address to listen on (`BOOMER_BIND`, default `0.0.0.0:8080`).
    pub bind_addr: String,
    /// Directory containing compiled Vite SPA assets (`BOOMER_STATIC_DIR`, default `web/dist`).
    pub static_dir: String,
    /// Public base URL used inside magic links (`BOOMER_PUBLIC_ORIGIN`, default `http://localhost:8080`).
    pub public_origin: String,
    /// Optional PostgreSQL connection string (`DATABASE_URL`). If omitted, uses in-memory dev repos.
    pub database_url: Option<String>,
    /// HMAC secret signing stateless session, magic-link, and guest tokens (`BOOMER_SESSION_SECRET`).
    pub session_secret: String,
    /// Optional SMTP config (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM`).
    pub smtp: Option<SmtpConfig>,
    /// Optional Google OAuth client ID (`GOOGLE_CLIENT_ID`).
    pub google_client_id: Option<String>,
    /// Optional Anthropic Claude API config (`ANTHROPIC_API_KEY`).
    pub anthropic: Option<AnthropicConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".to_owned(),
            static_dir: "web/dist".to_owned(),
            public_origin: "http://127.0.0.1:8080".to_owned(),
            database_url: None,
            session_secret: "dev-only-insecure-secret-change-in-production".to_owned(),
            smtp: None,
            google_client_id: None,
            anthropic: None,
        }
    }
}

impl ServerConfig {
    /// Reads configuration from environment variables with local-dev fallbacks.
    #[must_use]
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let bind_addr = std::env::var("BOOMER_BIND").unwrap_or(defaults.bind_addr);
        let static_dir = std::env::var("BOOMER_STATIC_DIR").unwrap_or(defaults.static_dir);
        let public_origin = std::env::var("BOOMER_PUBLIC_ORIGIN").unwrap_or(defaults.public_origin);
        let database_url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let session_secret =
            std::env::var("BOOMER_SESSION_SECRET").unwrap_or(defaults.session_secret);
        let google_client_id = std::env::var("GOOGLE_CLIENT_ID")
            .ok()
            .filter(|v| !v.trim().is_empty());

        let smtp = std::env::var("SMTP_HOST")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|host| SmtpConfig {
                host,
                port: std::env::var("SMTP_PORT")
                    .ok()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(587),
                username: std::env::var("SMTP_USER").unwrap_or_default(),
                password: std::env::var("SMTP_PASS").unwrap_or_default(),
                from: std::env::var("SMTP_FROM")
                    .unwrap_or_else(|_| "Boomergames <noreply@boomergames.local>".to_owned()),
            });

        let anthropic = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .map(|api_key| AnthropicConfig {
                api_key,
                model: std::env::var("ANTHROPIC_MODEL")
                    .ok()
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.to_owned()),
                max_tokens: std::env::var("ANTHROPIC_MAX_TOKENS")
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|&n| n > 0)
                    .unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS)
                    .clamp(32, 256),
                max_calls: std::env::var("ANTHROPIC_MAX_CALLS")
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|&n| n > 0)
                    .unwrap_or(DEFAULT_ANTHROPIC_MAX_CALLS),
            });

        Self {
            bind_addr,
            static_dir,
            public_origin,
            database_url,
            session_secret,
            smtp,
            google_client_id,
            anthropic,
        }
    }
}

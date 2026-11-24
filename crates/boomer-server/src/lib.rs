//! Authoritative Rust `axum` server for Boomergames.
//!
//! Single binary responsible for:
//! - Delivering the compiled Step 1/2 SPA (`web/dist/`) with strict CSP & caching
//! - Serving the cached `GET /status` health/metrics endpoint
//! - Handling `GET /ws` WebSocket connections, passwordless auth, lobbies, and tables

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod actors;
pub mod adapters;
pub mod auth;
pub mod config;
pub mod mapping;
pub mod ports;
pub mod state;

pub use actors::hub::LobbyHub;
pub use adapters::http::build_router;
pub use config::ServerConfig;
pub use state::AppState;

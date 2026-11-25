//! `axum` HTTP surface: SPA delivery (`GET /`), `/status`, and `/ws` upgrade.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::adapters::ws::ws_upgrade_handler;
use crate::state::{AppState, CachedStatus, StatusPayload};

/// Cache TTL for `GET /status` to avoid hammering PostgreSQL on health probes.
const STATUS_CACHE_TTL: Duration = Duration::from_secs(3);

/// Strict Content-Security-Policy matching `incubation/plan.md` §9.4 (+ Google Identity Services).
const STRICT_CSP: &str = "default-src 'self'; \
    script-src 'self' 'wasm-unsafe-eval' https://accounts.google.com; \
    style-src 'self' 'unsafe-inline' https://accounts.google.com; \
    connect-src 'self' ws: wss: https://accounts.google.com; \
    frame-src https://accounts.google.com; \
    img-src 'self' data: https://lh3.googleusercontent.com; \
    frame-ancestors 'none'";

/// Builds the complete `axum` [`Router`] exposing only the SPA, `/ws`, and `/status`.
pub fn build_router(state: AppState) -> Router {
    let static_root = PathBuf::from(&state.config.static_dir);
    let assets_dir = static_root.join("assets");
    let index_file = static_root.join("index.html");

    // Hashed assets under `/assets/*` are cached immutably for 1 year (§9.4).
    let assets_service = Router::new()
        .nest_service("/assets", ServeDir::new(assets_dir))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    // SPA fallback (`index.html` is `no-cache`).
    let spa_service = ServeDir::new(static_root).fallback(ServeFile::new(index_file));

    let csp_header = HeaderValue::from_static(STRICT_CSP);

    Router::new()
        .route("/status", get(status_handler))
        .route("/ws", get(ws_upgrade_handler))
        .merge(assets_service)
        .fallback_service(spa_service)
        .layer(SetResponseHeaderLayer::if_not_present(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            CONTENT_SECURITY_POLICY,
            csp_header,
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(CompressionLayer::new())
        .with_state(state)
}

async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let now = Instant::now();
    {
        let cache = state.status_cache.lock().await;
        if let Some(entry) = cache.as_ref()
            && now.duration_since(entry.fetched_at) < STATUS_CACHE_TTL
        {
            let code = if entry.healthy {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            return (code, Json(entry.payload.clone()));
        }
    }

    let db_ok = state.health.ping().await.is_ok();
    let payload = StatusPayload {
        status: if db_ok { "ok" } else { "degraded" }.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        uptime_s: state.started_at.elapsed().as_secs(),
        db: if db_ok { "ok" } else { "error" }.to_owned(),
        online: state.online.load(Ordering::Relaxed),
        tables: state.tables.load(Ordering::Relaxed),
        games: state.games.load(Ordering::Relaxed),
        google_client_id: state.config.google_client_id.clone(),
    };

    {
        let mut cache = state.status_cache.lock().await;
        *cache = Some(CachedStatus {
            fetched_at: now,
            healthy: db_ok,
            payload: payload.clone(),
        });
    }

    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(payload))
}

//! `axum` HTTP surface: SPA delivery (`GET /`), `/status`, and `/ws` upgrade.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
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
    worker-src 'self'; \
    style-src 'self' 'unsafe-inline' https://accounts.google.com; \
    connect-src 'self' ws: wss: https://accounts.google.com; \
    frame-src https://accounts.google.com; \
    img-src 'self' data: https://lh3.googleusercontent.com; \
    media-src 'self'; \
    frame-ancestors 'none'";

/// Plain-text bilingual privacy policy served at `GET /privacy` (required for Google OAuth).
const PRIVACY_POLICY: &str = "\
Boomergames — Privacy Policy / Datenschutzerklaerung
https://games.bleiglanz.info

ENGLISH
=======
Boomergames is a 100% free, non-commercial Checkers and 8-Ball Pool portal.

1. Data collected when you sign in (Google Sign-In or Email Magic Link):
   - Email address: used solely to identify your account and restore your
     nickname, ratings, and match history across logins.
   - Google account ID (sub) and initial suggested name (when using Google
     Sign-In): used to verify your Google login and pre-fill your initial
     nickname.
   - Nickname and built-in avatar choice: displayed to other players in the
     lounge and games. You may change your nickname on every login.
   - Game statistics: Elo ratings, win counts, and recent match summaries.

2. Guest play:
   - Playing as a guest requires no email or account. Only an ephemeral guest
     reconnect token and UI preferences (language, sound, high contrast) are
     stored locally in your browser's localStorage.

3. No cookies, no ads, no tracking:
   - Boomergames sets zero HTTP cookies, displays no ads, uses no analytics or
     tracking scripts, and never sells or shares your data with third parties.

4. Deleting your data:
   - You can permanently delete your account, email address, and statistics at
     any time from the Online Lounge: click your name -> Delete Account.

DEUTSCH
=======
Boomergames ist ein 100 % kostenloses, nicht-kommerzielles Dame- und
8-Ball-Billard-Portal.

1. Gespeicherte Daten bei Anmeldung (Google-Login oder E-Mail-Magic-Link):
   - E-Mail-Adresse: ausschliesslich zur Kontozuordnung sowie zum Wiederherstellen
     von Spitzname, Wertung und Spielverlauf bei erneuter Anmeldung.
   - Google-Konto-ID (sub) und Namensvorschlag (bei Google-Anmeldung): zur
     Verifizierung des Logins und als erster Spitznamen-Vorschlag.
   - Spitzname und Avatar: sichtbar fuer andere Spieler in der Lounge und im Spiel
     (bei jedem Login aenderbar).
   - Spielstatistiken: Elo-Wertungen, Siege und letzte Partien.

2. Keine Cookies, keine Werbung, kein Tracking:
   - Boomergames setzt keine Cookies, zeigt keine Werbung, nutzt keine
     Analyse- oder Tracking-Dienste und gibt keine Daten an Dritte weiter.

3. Loeschung deiner Daten:
   - Du kannst dein Konto samt E-Mail-Adresse und Statistiken jederzeit selbst
     dauerhaft loeschen: in der Online-Lounge auf deinen Namen klicken ->
     Konto loeschen.
";

/// Builds the complete `axum` [`Router`] exposing only the SPA, `/ws`, `/status`, and `/privacy`.
pub fn build_router(state: AppState) -> Router {
    let static_root = PathBuf::from(&state.config.static_dir);
    let assets_dir = static_root.join("assets");
    let audio_dir = static_root.join("audio");
    let stockfish_dir = static_root.join("stockfish");
    let index_file = static_root.join("index.html");

    // Hashed assets under `/assets/*` are cached immutably for 1 year (§9.4).
    let assets_service = Router::new()
        .nest_service("/assets", ServeDir::new(assets_dir))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    // Static CC-BY lounge jazz MP3s under `/audio/*` cached for 30 days.
    let audio_service = Router::new()
        .nest_service("/audio", ServeDir::new(audio_dir))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=2592000"),
        ));

    // Stockfish 19 WASM Web Worker assets under `/stockfish/*` cached for 30 days.
    let stockfish_service = Router::new()
        .nest_service("/stockfish", ServeDir::new(stockfish_dir))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=2592000"),
        ));

    // SPA fallback (`index.html` is `no-cache`).
    let spa_service = ServeDir::new(static_root).fallback(ServeFile::new(index_file));

    let csp_header = HeaderValue::from_static(STRICT_CSP);

    Router::new()
        .route("/status", get(status_handler))
        .route("/privacy", get(privacy_handler))
        .route("/ws", get(ws_upgrade_handler))
        .merge(assets_service)
        .merge(audio_service)
        .merge(stockfish_service)
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

async fn privacy_handler() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        PRIVACY_POLICY,
    )
}

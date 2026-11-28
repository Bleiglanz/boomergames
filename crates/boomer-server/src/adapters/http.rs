//! `axum` HTTP surface: SPA delivery (`GET /`), `/status`, and `/ws` upgrade.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, RETRY_AFTER, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::adapters::admin::{
    admin_ai_prompt_handler, admin_ai_reset_handler, admin_auth_google_handler,
    admin_ban_user_handler, admin_broadcast_handler, admin_close_table_handler,
    admin_delete_user_handler, admin_maintenance_handler, admin_moderator_handler,
    admin_mute_user_handler, admin_overview_handler, admin_resolve_report_handler,
    offline_play_telemetry_handler,
};
use crate::adapters::ws::ws_upgrade_handler;
use crate::state::{AppState, CachedStatus, StatusPayload};

/// Cache TTL for `GET /status` to avoid hammering PostgreSQL on health probes.
const STATUS_CACHE_TTL: Duration = Duration::from_secs(3);

/// Strict Content-Security-Policy matching `incubation/plan.md` §9.4 (+ Google Identity Services & `KaTeX` CDN).
const STRICT_CSP: &str = "default-src 'self'; \
    script-src 'self' 'wasm-unsafe-eval' https://accounts.google.com https://cdn.jsdelivr.net; \
    worker-src 'self'; \
    style-src 'self' 'unsafe-inline' https://accounts.google.com https://cdn.jsdelivr.net; \
    font-src 'self' https://cdn.jsdelivr.net; \
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
Boomergames is a 100% free, non-commercial 8-Ball Pool, Checkers, and Chess portal.

1. Data collected when you sign in (Google Sign-In or Email Magic Link):
   - Email address: used solely to identify your account and restore your
     nickname, ratings, and match history across logins.
   - Google account ID (sub) and initial suggested name (when using Google
     Sign-In): used to verify your Google login and pre-fill your initial
     nickname.
   - Nickname and built-in avatar choice: displayed to other players in the
     lounge and games. You may change your nickname on every login.
   - Game statistics: Elo ratings, win counts, and recent match summaries.

2. Ephemeral chat:
   - Lounge and table chat messages are held in memory only and automatically
     deleted after 24 hours (or sooner when a table closes or the server restarts).

3. Guest play:
   - Playing as a guest requires no email or account. Only an ephemeral guest
     reconnect token and UI preferences (language, sound, theme) are
     stored locally in your browser's localStorage.

4. No cookies, no ads, no tracking:
   - Boomergames sets zero HTTP cookies, displays no ads, uses no analytics or
     tracking scripts, and never sells or shares your data with third parties.

5. Deleting your data:
   - You can permanently delete your account, email address, and statistics at
     any time from the Online Lounge: click your name -> Delete Account.

DEUTSCH
=======
Boomergames ist ein 100 % kostenloses, nicht-kommerzielles 8-Ball-Billard-,
Dame- und Schach-Portal.

1. Gespeicherte Daten bei Anmeldung (Google-Login oder E-Mail-Magic-Link):
   - E-Mail-Adresse: ausschliesslich zur Kontozuordnung sowie zum Wiederherstellen
     von Spitzname, Wertung und Spielverlauf bei erneuter Anmeldung.
   - Google-Konto-ID (sub) und Namensvorschlag (bei Google-Anmeldung): zur
     Verifizierung des Logins und als erster Spitznamen-Vorschlag.
   - Spitzname und Avatar: sichtbar fuer andere Spieler in der Lounge und im Spiel
     (bei jedem Login aenderbar).
   - Spielstatistiken: Elo-Wertungen, Siege und letzte Partien.

2. Fluechtiger Chat:
   - Nachrichten im Raum- und Tisch-Chat werden nur im Arbeitsspeicher gehalten
     und automatisch nach 24 Stunden geloescht (oder frueher beim Schliessen
     eines Tisches bzw. Neustart des Servers).

3. Keine Cookies, keine Werbung, kein Tracking:
   - Boomergames setzt keine Cookies, zeigt keine Werbung, nutzt keine
     Analyse- oder Tracking-Dienste und gibt keine Daten an Dritte weiter.

4. Loeschung deiner Daten:
   - Du kannst dein Konto samt E-Mail-Adresse und Statistiken jederzeit selbst
     dauerhaft loeschen: in der Online-Lounge auf deinen Namen klicken ->
     Konto loeschen.
";

/// Plain-text bilingual About / Impressum page served at `GET /about` and `GET /impressum`.
const IMPRESSUM_ABOUT: &str = "\
Boomergames — About / Impressum
https://games.bleiglanz.info

ENGLISH (ABOUT)
===============
Boomergames is a private, non-commercial hobby project built for fun and
nostalgia — a 100% free, ad-free, cookie-free classic 8-Ball Pool, Checkers,
and Chess lounge inspired by classic 2000s browser games.

Creator & Contact:
  Anton Rechenauer
  Email: antonrechenauer@gmail.com

Open Source (Fork It!):
  https://github.com/Bleiglanz/boomergames
  Everyone is warmly invited to explore the source code, fork the repository,
  tinker with it, add new games, or host their own Boomergames lounge!

DEUTSCH (IMPRESSUM / UEBER)
===========================
Boomergames ist ein privates, rein nicht-kommerzielles Hobbyprojekt aus Spass
und Nostalgie — eine 100 % kostenlose, werbe- und cookiefreie Lounge fuer
8-Ball Billard, Dame und Schach im Stil klassischer 2000er-Browserspiele.

Verantwortlich & Kontakt:
  Anton Rechenauer
  E-Mail: antonrechenauer@gmail.com

Open Source (Gerne forken!):
  https://github.com/Bleiglanz/boomergames
  Alle sind herzlich eingeladen, den Quellcode anzusehen, das Repository zu
  forken, eigene Ideen auszuprobieren oder eine eigene Instanz zu betreiben!
";

/// Builds the complete `axum` [`Router`] exposing only the SPA, `/ws`, `/status`, `/privacy`,
/// `/about`, and `/impressum`.
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

    // SPA fallback (`index.html` is `no-cache`), gated by Maintenance Mode.
    let spa_service = ServeDir::new(static_root).fallback(ServeFile::new(index_file.clone()));
    let spa_router =
        Router::new()
            .fallback_service(spa_service)
            .layer(middleware::from_fn_with_state(
                state.clone(),
                maintenance_gate,
            ));

    // Dedicated `/antonadmin` SPA shell route that stays reachable during Maintenance Mode
    // so the Google-authenticated admin can always log in and resume the site.
    let admin_page_service = ServeFile::new(index_file);

    let csp_header = HeaderValue::from_static(STRICT_CSP);

    Router::new()
        .route("/status", get(status_handler))
        .route("/privacy", get(privacy_handler))
        .route("/about", get(impressum_handler))
        .route("/impressum", get(impressum_handler))
        .route("/ws", get(ws_upgrade_handler))
        .route(
            "/api/telemetry/offline",
            post(offline_play_telemetry_handler),
        )
        .route_service("/antonadmin", admin_page_service.clone())
        .route_service("/antonadmin/", admin_page_service)
        .route(
            "/antonadmin/api/auth/google",
            post(admin_auth_google_handler),
        )
        .route("/antonadmin/api/overview", get(admin_overview_handler))
        .route(
            "/antonadmin/api/maintenance",
            post(admin_maintenance_handler),
        )
        .route("/antonadmin/api/broadcast", post(admin_broadcast_handler))
        .route("/antonadmin/api/users/mute", post(admin_mute_user_handler))
        .route("/antonadmin/api/users/ban", post(admin_ban_user_handler))
        .route(
            "/antonadmin/api/users/moderator",
            post(admin_moderator_handler),
        )
        .route(
            "/antonadmin/api/users/delete",
            post(admin_delete_user_handler),
        )
        .route(
            "/antonadmin/api/tables/close",
            post(admin_close_table_handler),
        )
        .route(
            "/antonadmin/api/reports/resolve",
            post(admin_resolve_report_handler),
        )
        .route("/antonadmin/api/ai/reset", post(admin_ai_reset_handler))
        .route("/antonadmin/api/ai/prompt", post(admin_ai_prompt_handler))
        .merge(assets_service)
        .merge(audio_service)
        .merge(stockfish_service)
        .merge(spa_router)
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

async fn maintenance_gate(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.is_maintenance() {
        let escaped = html_escape(&state.maintenance_message());
        let body = format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>Boomergames — Maintenance</title>\
             <style>body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;\
             background:#cdd9dc;color:#182226;font-family:Verdana,Tahoma,sans-serif;padding:1.5rem}}\
             .card{{max-width:520px;background:#f8f6ec;border:2px solid #4f6469;padding:1.5rem 1.75rem}}\
             h1{{margin:0 0 .65rem;font-size:1.35rem;color:#215b6d}}p{{margin:.45rem 0;line-height:1.5}}\
             .muted{{color:#405359;font-size:.9rem}}</style></head>\
             <body><main class=\"card\"><h1>Boomergames — Maintenance Mode / Wartungsmodus</h1>\
             <p>{escaped}</p>\
             <p class=\"muted\">Please check back shortly. / Bitte schau gleich wieder vorbei.</p>\
             </main></body></html>"
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (CONTENT_TYPE, "text/html; charset=utf-8"),
                (RETRY_AFTER, "120"),
            ],
            body,
        )
            .into_response();
    }
    next.run(req).await
}

fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
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
        ai_friend_available: state.ai_friend.is_configured(),
        maintenance: state.is_maintenance(),
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

async fn impressum_handler() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        IMPRESSUM_ABOUT,
    )
}

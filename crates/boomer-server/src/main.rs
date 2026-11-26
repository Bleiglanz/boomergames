//! Binary entrypoint for `boomer-server`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use boomer_server::adapters::db::PgRepos;
use boomer_server::adapters::google::{GoogleIdTokenVerifier, UnconfiguredGoogleVerifier};
use boomer_server::adapters::mail::{LettreMailSender, LogMailSender};
use boomer_server::adapters::memory::MemoryRepos;
use boomer_server::auth::TokenSigner;
use boomer_server::ports::{
    GoogleTokenVerifier, HealthCheck, MailSender, MatchRepo, ReportRepo, StatsRepo, UserRepo,
};
use boomer_server::{AppState, ServerConfig, build_router};
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

struct RepoBundle {
    users: Arc<dyn UserRepo>,
    stats: Arc<dyn StatsRepo>,
    matches: Arc<dyn MatchRepo>,
    reports: Arc<dyn ReportRepo>,
    health: Arc<dyn HealthCheck>,
}

#[tokio::main]
async fn main() {
    init_tracing();
    let config = ServerConfig::from_env();

    let repos = if let Some(ref db_url) = config.database_url {
        match PgRepos::connect_and_migrate(db_url).await {
            Ok(pg) => {
                info!("connected to PostgreSQL and applied migrations");
                let pg = Arc::new(pg);
                RepoBundle {
                    users: pg.clone(),
                    stats: pg.clone(),
                    matches: pg.clone(),
                    reports: pg.clone(),
                    health: pg,
                }
            }
            Err(err) => {
                error!(%err, "failed to initialize PostgreSQL pool");
                return;
            }
        }
    } else {
        warn!("DATABASE_URL not set; running with in-memory development store");
        let mem = Arc::new(MemoryRepos::new());
        RepoBundle {
            users: mem.clone(),
            stats: mem.clone(),
            matches: mem.clone(),
            reports: mem.clone(),
            health: mem,
        }
    };

    let mailer: Arc<dyn MailSender> = if let Some(smtp) = &config.smtp {
        match LettreMailSender::new(smtp) {
            Ok(sender) => {
                info!(host = %smtp.host, port = smtp.port, "configured SMTP magic-link mailer");
                Arc::new(sender)
            }
            Err(err) => {
                error!(%err, "failed to configure SMTP mailer");
                return;
            }
        }
    } else {
        warn!("SMTP_HOST not set; magic links will be logged to stdout/tracing");
        Arc::new(LogMailSender)
    };

    let google: Arc<dyn GoogleTokenVerifier> = config.google_client_id.as_ref().map_or_else(
        || {
            warn!("GOOGLE_CLIENT_ID not set; Google sign-in disabled");
            Arc::new(UnconfiguredGoogleVerifier) as Arc<dyn GoogleTokenVerifier>
        },
        |client_id| Arc::new(GoogleIdTokenVerifier::new(client_id.clone())),
    );

    let signer = Arc::new(TokenSigner::new(config.session_secret.as_bytes()));
    let state = AppState::new(
        config.clone(),
        signer,
        repos.users,
        repos.stats,
        repos.matches,
        repos.reports,
        repos.health,
        mailer,
        google,
    );

    let drain_hub = Arc::clone(&state.hub);
    let router = build_router(state);
    let listener = match TcpListener::bind(&config.bind_addr).await {
        Ok(l) => l,
        Err(err) => {
            error!(bind = %config.bind_addr, %err, "failed to bind TCP listener");
            return;
        }
    };

    info!(bind = %config.bind_addr, static_dir = %config.static_dir, "boomer-server listening");

    if let Err(err) = axum::serve(listener, router)
        .with_graceful_shutdown(graceful_drain_signal(drain_hub))
        .await
    {
        error!(%err, "axum server terminated with error");
    }
}

async fn graceful_drain_signal(hub: Arc<boomer_server::LobbyHub>) {
    shutdown_signal().await;
    info!("drain signal received: broadcasting notice and waiting for running games (max 15 min)");
    hub.begin_drain();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    while hub.games_count() > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let disconnected = hub.disconnect_all_clients();
    info!(
        remaining_games = hub.games_count(),
        disconnected_clients = disconnected,
        "drain complete; shutting down"
    );
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json_logs =
        std::env::var("BOOMER_LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json"));

    if json_logs {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            let _ = sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received SIGINT; shutting down"),
        () = terminate => info!("received SIGTERM; shutting down"),
    }
}

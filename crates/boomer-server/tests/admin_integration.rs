//! Integration tests for `/antonadmin` strict Google-only auth, telemetry, moderation, and Maintenance Mode.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

use std::sync::Arc;

use boomer_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use boomer_server::adapters::memory::{CaptureMailSender, MemoryRepos, TestGoogleVerifier};
use boomer_server::auth::{ADMIN_EMAIL, AuthProvider, TokenSigner};
use boomer_server::ports::UserRepo;
use boomer_server::{AppState, ServerConfig, build_router};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;

struct AdminTestServer {
    base_http: String,
    base_ws: String,
    repos: Arc<MemoryRepos>,
    signer: Arc<TokenSigner>,
    tmp_static: std::path::PathBuf,
}

impl Drop for AdminTestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp_static);
    }
}

async fn spawn_admin_server() -> AdminTestServer {
    let tmp_static = std::env::temp_dir().join(format!(
        "boomer-admin-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let _ = std::fs::create_dir_all(&tmp_static);
    std::fs::write(
        tmp_static.join("index.html"),
        "<!doctype html><div id=\"app\">BoomerTest</div>",
    )
    .unwrap();

    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"admin-integration-test-secret-32b"));
    let config = ServerConfig {
        public_origin: "https://games.bleiglanz.info".to_owned(),
        static_dir: tmp_static.to_string_lossy().into_owned(),
        google_client_id: Some("test-google-client-id".to_owned()),
        ..ServerConfig::default()
    };

    let state = AppState::new(
        config,
        Arc::clone(&signer),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        mailer,
        google,
    )
    .with_admin_repo(repos.clone());

    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    AdminTestServer {
        base_http: format!("http://{addr}"),
        base_ws: format!("ws://{addr}/ws"),
        repos,
        signer,
        tmp_static,
    }
}

#[tokio::test]
async fn admin_api_is_strictly_forbidden_except_google_authenticated_anton() {
    let srv = spawn_admin_server().await;
    let client = reqwest::Client::new();

    // 1. Unauthenticated request -> 403 Forbidden.
    let resp = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    // 2. Google sign-in with ANOTHER Gmail account -> 403 Forbidden.
    let other_resp = client
        .post(format!("{}/antonadmin/api/auth/google", srv.base_http))
        .json(&serde_json::json!({ "idToken": "valid:sub-other:someone@gmail.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(other_resp.status(), 403);

    // 3. Even if ADMIN_EMAIL authenticates via Email Magic Link (not Google), session token is 403 Forbidden!
    let (anton_user, _) = srv
        .repos
        .find_or_create_by_email(ADMIN_EMAIL, Some("sub-anton"), Some("Anton"))
        .await
        .unwrap();
    let email_only_token = srv
        .signer
        .sign_session_with_provider(
            anton_user.id,
            anton_user.session_version,
            AuthProvider::Email,
        )
        .unwrap();
    let email_resp = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .bearer_auth(&email_only_token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        email_resp.status(),
        403,
        "magic-link session token must NOT grant /antonadmin access"
    );

    // 4. Google-authenticated session token for another user -> 403 Forbidden.
    let (bob, _) = srv
        .repos
        .find_or_create_by_email("bob@gmail.com", Some("sub-bob"), Some("Bob"))
        .await
        .unwrap();
    let bob_google_session = srv
        .signer
        .sign_session_with_provider(bob.id, bob.session_version, AuthProvider::Google)
        .unwrap();
    let bob_resp = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .bearer_auth(&bob_google_session)
        .send()
        .await
        .unwrap();
    assert_eq!(bob_resp.status(), 403);

    // 5. Google-authenticated session token for antonrechenauer@gmail.com -> 200 OK!
    let anton_google_session = srv
        .signer
        .sign_session_with_provider(
            anton_user.id,
            anton_user.session_version,
            AuthProvider::Google,
        )
        .unwrap();
    let ok_session_resp = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .bearer_auth(&anton_google_session)
        .send()
        .await
        .unwrap();
    assert_eq!(ok_session_resp.status(), 200);

    // 6. Direct Google ID token exchange for antonrechenauer@gmail.com -> 200 OK + working admin token!
    let auth_resp = client
        .post(format!("{}/antonadmin/api/auth/google", srv.base_http))
        .json(&serde_json::json!({
            "idToken": format!("valid:sub-anton:{ADMIN_EMAIL}")
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(auth_resp.status(), 200);
    let auth_json: serde_json::Value = auth_resp.json().await.unwrap();
    let admin_token = auth_json["token"].as_str().unwrap().to_owned();

    let overview_resp = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .bearer_auth(&admin_token)
        .send()
        .await
        .unwrap();
    assert_eq!(overview_resp.status(), 200);
    let overview: serde_json::Value = overview_resp.json().await.unwrap();
    assert_eq!(overview["adminEmail"], ADMIN_EMAIL);
    assert_eq!(overview["users"].as_array().unwrap().len(), 2);
}

async fn obtain_admin_token(client: &reqwest::Client, base_http: &str) -> String {
    let auth_json: serde_json::Value = client
        .post(format!("{base_http}/antonadmin/api/auth/google"))
        .json(&serde_json::json!({
            "idToken": format!("valid:sub-anton:{ADMIN_EMAIL}")
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    auth_json["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn maintenance_mode_stops_public_site_and_ws_while_keeping_admin_reachable() {
    let srv = spawn_admin_server().await;
    let client = reqwest::Client::new();
    let admin_token = obtain_admin_token(&client, &srv.base_http).await;

    // Record a guest WebSocket connect and an anonymous offline play.
    let (mut ws, _) = connect_async(&srv.base_ws).await.unwrap();
    let hello = serde_json::to_string(&ClientMessage::Hello {
        v: PROTOCOL_VERSION,
        session: None,
        guest: None,
    })
    .unwrap();
    ws.send(WsMsg::Text(hello.into())).await.unwrap();
    if let WsMsg::Text(txt) = ws.next().await.unwrap().unwrap() {
        assert!(matches!(
            serde_json::from_str::<ServerMessage>(&txt).unwrap(),
            ServerMessage::Welcome { .. }
        ));
    }
    let telemetry = client
        .post(format!("{}/api/telemetry/offline", srv.base_http))
        .json(&serde_json::json!({ "game": "chess", "mode": "vs_bot", "level": "medium" }))
        .send()
        .await
        .unwrap();
    assert_eq!(telemetry.status(), 204);
    assert_eq!(
        client
            .get(format!("{}/", srv.base_http))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Enable Maintenance Mode (with disconnectClients = true).
    let maint_body: serde_json::Value = client
        .post(format!("{}/antonadmin/api/maintenance", srv.base_http))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "enabled": true,
            "message": "Upgrading tables — back in 5 minutes!",
            "disconnectClients": true
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(maint_body["maintenance"], true);
    assert_eq!(maint_body["disconnectedClients"], 1);

    // Public `/` returns 503 Maintenance HTML and `/ws` is refused.
    let root_during = client
        .get(format!("{}/", srv.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(root_during.status(), 503);
    assert!(
        root_during
            .text()
            .await
            .unwrap()
            .contains("Upgrading tables")
    );
    assert!(connect_async(&srv.base_ws).await.is_err());

    // `/antonadmin` shell and `/status` stay reachable during Maintenance Mode.
    assert_eq!(
        client
            .get(format!("{}/antonadmin", srv.base_http))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let status_during: serde_json::Value = client
        .get(format!("{}/status", srv.base_http))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status_during["maintenance"], true);

    // Activity log recorded both the guest connect and the offline play.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let overview: serde_json::Value = client
        .get(format!("{}/antonadmin/api/overview", srv.base_http))
        .bearer_auth(&admin_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let events = overview["activity"].as_array().unwrap();
    assert!(events.iter().any(|e| e["eventType"] == "guest_connect"));
    assert!(
        events
            .iter()
            .any(|e| e["eventType"] == "offline_play" && e["game"] == "chess")
    );

    // Disable Maintenance Mode -> site & WebSocket immediately recover.
    let maint_off = client
        .post(format!("{}/antonadmin/api/maintenance", srv.base_http))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({ "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(maint_off.status(), 200);

    let root_after = client
        .get(format!("{}/", srv.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(root_after.status(), 200);
    assert!(connect_async(&srv.base_ws).await.is_ok());
}

//! End-to-end TCP + WebSocket integration tests for Milestone 4 (`boomer-server`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use boomer_domain::GameKind;
use boomer_protocol::{
    ClientMessage, ErrorCode, GameKindDto, PROTOCOL_VERSION, ServerMessage, SessionKindDto,
};
use boomer_server::adapters::memory::{CaptureMailSender, MemoryRepos, TestGoogleVerifier};
use boomer_server::auth::TokenSigner;
use boomer_server::ports::PlayerStatRecord;
use boomer_server::{AppState, ServerConfig, build_router};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;

struct TestServer {
    base_http: String,
    base_ws: String,
    repos: Arc<MemoryRepos>,
    mailer: Arc<CaptureMailSender>,
}

async fn spawn_test_server_with_static(static_dir: String) -> TestServer {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"integration-test-secret-32-bytes!"));
    let config = ServerConfig {
        public_origin: "https://boomergames.test".to_owned(),
        static_dir,
        ..ServerConfig::default()
    };

    let state = AppState::new(
        config,
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        mailer.clone(),
        google,
    );
    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    TestServer {
        base_http: format!("http://{addr}"),
        base_ws: format!("ws://{addr}/ws"),
        repos,
        mailer,
    }
}

async fn spawn_test_server() -> TestServer {
    spawn_test_server_with_static("../../web/dist".to_owned()).await
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send_json(ws: &mut WsStream, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(WsMsg::Text(text.into())).await.unwrap();
}

async fn recv_json(ws: &mut WsStream) -> ServerMessage {
    loop {
        let frame = ws
            .next()
            .await
            .expect("socket closed unexpectedly")
            .unwrap();
        if let WsMsg::Text(text) = frame {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

async fn connect_hello(
    srv: &TestServer,
    session: Option<String>,
    guest: Option<String>,
) -> (WsStream, ServerMessage) {
    let (mut ws, _) = connect_async(&srv.base_ws).await.unwrap();
    send_json(
        &mut ws,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session,
            guest,
        },
    )
    .await;
    let welcome = recv_json(&mut ws).await;
    (ws, welcome)
}

async fn login_with_magic_link(srv: &TestServer, email: &str) -> (WsStream, String, String) {
    let (mut ws, _) = connect_hello(srv, None, None).await;
    send_json(
        &mut ws,
        &ClientMessage::AuthEmail {
            email: email.to_owned(),
        },
    )
    .await;
    assert_eq!(recv_json(&mut ws).await, ServerMessage::AuthSent {});
    let sent = srv.mailer.sent_messages();
    let magic_token = sent
        .last()
        .expect("magic email must be captured")
        .1
        .split("#login=")
        .nth(1)
        .expect("magic link must contain #login=")
        .to_owned();
    send_json(&mut ws, &ClientMessage::AuthMagic { token: magic_token }).await;
    match recv_json(&mut ws).await {
        ServerMessage::AuthOk {
            session,
            me,
            is_new,
        } => {
            assert!(is_new);
            assert!(!me.guest);
            (ws, session, me.id)
        }
        other => panic!("expected AuthOk, got {other:?}"),
    }
}

#[tokio::test]
async fn status_endpoint_reports_health_and_degradation() {
    let srv = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/status", srv.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("content-security-policy"));
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], "ok");
    assert_eq!(body["online"], 0);

    // A degraded backing store returns HTTP 503.
    let degraded = spawn_test_server().await;
    degraded.repos.set_healthy(false);
    let bad_resp = client
        .get(format!("{}/status", degraded.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_resp.status(), 503);
    let bad_body: serde_json::Value = bad_resp.json().await.unwrap();
    assert_eq!(bad_body["status"], "degraded");
    assert_eq!(bad_body["db"], "error");

    // Static SPA serving includes strict CSP + X-Content-Type-Options.
    let tmp = std::env::temp_dir().join(format!("boomer-spa-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(tmp.join("index.html"), "<!doctype html><h1>Boomer</h1>").unwrap();
    let spa_srv = spawn_test_server_with_static(tmp.to_string_lossy().into_owned()).await;
    let spa_resp = client
        .get(format!("{}/", spa_srv.base_http))
        .send()
        .await
        .unwrap();
    assert_eq!(spa_resp.status(), 200);
    let csp = spa_resp
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("frame-ancestors 'none'"));
    assert_eq!(
        spa_resp
            .headers()
            .get("x-content-type-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "nosniff"
    );
    assert!(spa_resp.text().await.unwrap().contains("Boomer"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn guest_reconnect_preserves_identity() {
    let srv = spawn_test_server().await;

    // Client A connects as Guest and preserves ID on reconnect via signed guest token.
    let (mut ws_first, _) = connect_async(&srv.base_ws).await.unwrap();
    send_json(
        &mut ws_first,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    let (guest_id, guest_token) = match recv_json(&mut ws_first).await {
        ServerMessage::Welcome {
            mode,
            me: Some(me),
            guest: Some(tok),
            rooms,
            ..
        } => {
            assert_eq!(mode, SessionKindDto::Remote);
            assert!(me.guest);
            assert!(me.name.starts_with("Guest_"));
            assert_eq!(rooms.len(), 2);
            (me.id, tok)
        }
        other => panic!("unexpected welcome: {other:?}"),
    };
    let _ = ws_first.close(None).await;

    // Client A reconnects with guest token -> keeps identical Guest_XXXX id.
    let (mut ws_reconnected, _) = connect_async(&srv.base_ws).await.unwrap();
    send_json(
        &mut ws_reconnected,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: Some(guest_token),
        },
    )
    .await;
    match recv_json(&mut ws_reconnected).await {
        ServerMessage::Welcome { me: Some(me), .. } => assert_eq!(me.id, guest_id),
        other => panic!("unexpected guest reconnect welcome: {other:?}"),
    }
}

#[tokio::test]
async fn magic_link_profile_highscores_and_gdpr_delete() {
    let srv = spawn_test_server().await;
    let (mut ws_main, session_token, user_wire_id) =
        login_with_magic_link(&srv, "Alice@Example.com").await;

    // Update display name & avatar.
    send_json(
        &mut ws_main,
        &ClientMessage::AccountUpdate {
            name: Some("RailMaster".to_owned()),
            avatar: Some(4),
        },
    )
    .await;
    match recv_json(&mut ws_main).await {
        ServerMessage::AuthOk { me, .. } => {
            assert_eq!(me.name, "RailMaster");
            assert_eq!(me.avatar, 4);
        }
        other => panic!("expected updated AuthOk, got {other:?}"),
    }

    // Immediate second rename is blocked by 30-day cooldown.
    send_json(
        &mut ws_main,
        &ClientMessage::AccountUpdate {
            name: Some("AnotherName".to_owned()),
            avatar: None,
        },
    )
    .await;
    assert!(matches!(
        recv_json(&mut ws_main).await,
        ServerMessage::Error {
            code: ErrorCode::Invalid,
            ..
        }
    ));

    // Seed a Pool stat row and verify Profile + Highscores over WS.
    let uid: i64 = user_wire_id.strip_prefix('u').unwrap().parse().unwrap();
    srv.repos
        .upsert_stat(PlayerStatRecord {
            user_id: uid,
            game: GameKind::Pool,
            rating: 1420,
            rated_games: 12,
            games: 15,
            wins: 9,
            wins_reached_at: Some(Utc::now()),
        })
        .unwrap();

    send_json(
        &mut ws_main,
        &ClientMessage::ProfileGet {
            user_id: user_wire_id.clone(),
        },
    )
    .await;
    match recv_json(&mut ws_main).await {
        ServerMessage::Profile { user, stats, .. } => {
            assert_eq!(user.name, "RailMaster");
            assert_eq!(stats.len(), 1);
            assert_eq!(stats[0].rating, 1420);
        }
        other => panic!("expected Profile, got {other:?}"),
    }

    send_json(
        &mut ws_main,
        &ClientMessage::HighscoresGet {
            game: GameKindDto::Pool,
            around: true,
        },
    )
    .await;
    match recv_json(&mut ws_main).await {
        ServerMessage::Highscores { game, rows } => {
            assert_eq!(game, GameKindDto::Pool);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "RailMaster");
            assert_eq!(rows[0].wins, 9);
        }
        other => panic!("expected Highscores, got {other:?}"),
    }

    // Reconnect using stateless session token -> authenticated immediately.
    let (mut ws_resumed, welcome) = connect_hello(&srv, Some(session_token.clone()), None).await;
    match welcome {
        ServerMessage::Welcome { me: Some(me), .. } => {
            assert_eq!(me.id, user_wire_id);
            assert_eq!(me.name, "RailMaster");
        }
        other => panic!("expected session welcome, got {other:?}"),
    }

    // GDPR Account Delete sends a clean Close frame and deletes the user.
    send_json(&mut ws_resumed, &ClientMessage::AccountDelete {}).await;
    let next_frame = ws_resumed.next().await;
    assert!(matches!(next_frame, None | Some(Ok(WsMsg::Close(_)))));

    // Old session token now falls back to a fresh guest (user row is gone).
    let (_ws_after, after_welcome) = connect_hello(&srv, Some(session_token), None).await;
    match after_welcome {
        ServerMessage::Welcome { me: Some(me), .. } => assert!(me.guest),
        other => panic!("expected guest fallback after delete, got {other:?}"),
    }
}

#[tokio::test]
async fn google_sign_in_and_protocol_version_guard_work() {
    let srv = spawn_test_server().await;

    // Wrong protocol version receives `upgrade_required`.
    let (mut ws_bad, _) = connect_async(&srv.base_ws).await.unwrap();
    send_json(
        &mut ws_bad,
        &ClientMessage::Hello {
            v: 999,
            session: None,
            guest: None,
        },
    )
    .await;
    assert_eq!(
        recv_json(&mut ws_bad).await,
        ServerMessage::Error {
            code: ErrorCode::UpgradeRequired,
            re: "hello".to_owned(),
        }
    );

    // Google sign-in via TestGoogleVerifier succeeds.
    let (mut ws, _) = connect_async(&srv.base_ws).await.unwrap();
    send_json(
        &mut ws,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        &ClientMessage::AuthGoogle {
            id_token: "valid:sub-999:bob@example.com".to_owned(),
        },
    )
    .await;
    match recv_json(&mut ws).await {
        ServerMessage::AuthOk { me, is_new, .. } => {
            assert!(is_new);
            assert!(!me.guest);
        }
        other => panic!("expected Google AuthOk, got {other:?}"),
    }
}

//! Milestone 7 integration & load tests:
//! - 45s `PAUSED_DISCONNECT` grace period, full snapshot reconnect, and expiry forfeit
//! - Moderation (`report` with chat log, `mod.mute`, `mod.ban` with `session_version` bump, `mod.close_table`)
//! - Graceful drain refusal of new games & system broadcast
//! - 200-concurrent-user live WebSocket load test

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

use std::sync::Arc;
use std::time::Duration;

use boomer_protocol::{
    ChatScopeDto, ClientMessage, ErrorCode, KickReasonDto, PROTOCOL_VERSION, SeatDto,
    ServerMessage, TableRoleDto, TableSettingsDto,
};
use boomer_server::adapters::memory::{CaptureMailSender, MemoryRepos, TestGoogleVerifier};
use boomer_server::auth::TokenSigner;
use boomer_server::{AppState, ServerConfig, build_router};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct TestServer {
    base_http: String,
    base_ws: String,
    repos: Arc<MemoryRepos>,
    mailer: Arc<CaptureMailSender>,
    state: AppState,
}

async fn spawn_server(disconnect_grace: Duration) -> TestServer {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"milestone-7-resilience-secret!!!"));
    let state = AppState::new(
        ServerConfig::default(),
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        mailer.clone(),
        google,
    );
    state.hub.set_disconnect_grace(disconnect_grace);

    let router = build_router(state.clone());
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
        state,
    }
}

async fn send_msg(ws: &mut WsStream, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(WsMsg::Text(text.into())).await.unwrap();
}

async fn recv_until<F, T>(ws: &mut WsStream, mut pred: F) -> T
where
    F: FnMut(ServerMessage) -> Option<T>,
{
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => panic!("timed out waiting for expected ServerMessage"),
            frame = ws.next() => {
                let frame = frame.expect("socket closed unexpectedly").unwrap();
                if let WsMsg::Text(text) = frame {
                    let parsed: ServerMessage = serde_json::from_str(&text).unwrap();
                    if let Some(found) = pred(parsed) {
                        return found;
                    }
                }
            }
        }
    }
}

async fn connect_registered(
    srv: &TestServer,
    email: &str,
    room: &str,
) -> (WsStream, String, String) {
    let (mut ws, _) = connect_async(&srv.base_ws).await.unwrap();
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    recv_until(&mut ws, |m| {
        matches!(m, ServerMessage::Welcome { .. }).then_some(())
    })
    .await;

    send_msg(
        &mut ws,
        &ClientMessage::AuthEmail {
            email: email.to_owned(),
        },
    )
    .await;
    recv_until(&mut ws, |m| {
        matches!(m, ServerMessage::AuthSent {}).then_some(())
    })
    .await;
    let magic = srv
        .mailer
        .sent_messages()
        .into_iter()
        .rev()
        .find(|(to, _)| to.eq_ignore_ascii_case(email))
        .unwrap()
        .1
        .split("#login=")
        .nth(1)
        .unwrap()
        .to_owned();
    send_msg(&mut ws, &ClientMessage::AuthMagic { token: magic }).await;
    let (session, wire_id) = recv_until(&mut ws, |m| match m {
        ServerMessage::AuthOk { session, me, .. } => Some((session, me.id)),
        _ => None,
    })
    .await;

    send_msg(
        &mut ws,
        &ClientMessage::LobbyJoin {
            room: room.to_owned(),
        },
    )
    .await;
    let _ = recv_until(&mut ws, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    (ws, wire_id, session)
}

#[tokio::test]
async fn disconnect_pauses_match_and_reconnect_resumes_with_full_snapshot() {
    let srv = spawn_server(Duration::from_secs(10)).await;
    let (mut alice, _, alice_session) =
        connect_registered(&srv, "dc1@example.com", "checkers-social-1").await;
    let (mut bob, _, _) = connect_registered(&srv, "dc2@example.com", "checkers-social-1").await;

    send_msg(
        &mut alice,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_until(&mut alice, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s.id),
        _ => None,
    })
    .await;
    send_msg(
        &mut bob,
        &ClientMessage::TableJoin {
            table_id: table_id.clone(),
            role: TableRoleDto::Player,
        },
    )
    .await;
    let _ = recv_until(&mut bob, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    send_msg(&mut alice, &ClientMessage::Ready {}).await;
    send_msg(&mut bob, &ClientMessage::Ready {}).await;
    let _ = recv_until(&mut alice, |m| match m {
        ServerMessage::CheckersSnapshot { state } => Some(state),
        _ => None,
    })
    .await;

    // Alice's socket drops -> Bob receives GamePause { seat: 1 }!
    drop(alice);
    let paused_seat = recv_until(&mut bob, |m| match m {
        ServerMessage::GamePause { seat, .. } => Some(seat),
        _ => None,
    })
    .await;
    assert_eq!(paused_seat, SeatDto::One);

    // Alice reconnects with her session token -> automatically receives TableSnapshot + CheckersSnapshot,
    // and Bob receives GameResume!
    let (mut alice_reconnected, _) = connect_async(&srv.base_ws).await.unwrap();
    send_msg(
        &mut alice_reconnected,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: Some(alice_session),
            guest: None,
        },
    )
    .await;
    let restored_table = recv_until(&mut alice_reconnected, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    assert_eq!(restored_table.id, table_id);
    let _ = recv_until(&mut alice_reconnected, |m| match m {
        ServerMessage::CheckersSnapshot { state } => Some(state),
        _ => None,
    })
    .await;
    recv_until(&mut bob, |m| {
        matches!(m, ServerMessage::GameResume {}).then_some(())
    })
    .await;
}

#[tokio::test]
async fn disconnect_grace_expiry_forfeits_match_and_updates_elo() {
    let srv = spawn_server(Duration::from_millis(300)).await;
    let (mut alice, _, _) = connect_registered(&srv, "exp1@example.com", "checkers-social-1").await;
    let (mut bob, bob_id, _) =
        connect_registered(&srv, "exp2@example.com", "checkers-social-1").await;

    send_msg(
        &mut alice,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_until(&mut alice, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s.id),
        _ => None,
    })
    .await;
    send_msg(
        &mut bob,
        &ClientMessage::TableJoin {
            table_id,
            role: TableRoleDto::Player,
        },
    )
    .await;
    let _ = recv_until(&mut bob, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    send_msg(&mut alice, &ClientMessage::Ready {}).await;
    send_msg(&mut bob, &ClientMessage::Ready {}).await;
    let _ = recv_until(&mut bob, |m| match m {
        ServerMessage::CheckersSnapshot { state } => Some(state),
        _ => None,
    })
    .await;

    drop(alice);
    let over = recv_until(&mut bob, |m| match m {
        ServerMessage::GameOver(g) => Some(g),
        _ => None,
    })
    .await;
    assert_eq!(over.winner, Some(SeatDto::Two));
    assert_eq!(over.reason, "disconnect");
    assert!(over.rating.iter().any(|r| r.id == bob_id && r.new > 1200));
}

#[tokio::test]
async fn moderation_report_mute_ban_close_table_and_drain_work() {
    let srv = spawn_server(Duration::from_secs(45)).await;
    let (mut mod_ws, mod_id, _) =
        connect_registered(&srv, "mod@example.com", "pool-social-1").await;
    let (mut target_ws, target_id, target_session) =
        connect_registered(&srv, "bad@example.com", "pool-social-1").await;

    // Non-moderator cannot call mod.mute.
    send_msg(
        &mut target_ws,
        &ClientMessage::ModMute {
            user_id: mod_id.clone(),
            minutes: 10,
        },
    )
    .await;
    let denied = recv_until(&mut target_ws, |m| match m {
        ServerMessage::Error { code, re } if re == "mod.mute" => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(denied, ErrorCode::NotAllowed);

    // Promote mod_ws to moderator in repo.
    let moderator_pk: i64 = mod_id.strip_prefix('u').unwrap().parse().unwrap();
    srv.repos.set_moderator(moderator_pk, true).unwrap();

    // Target chats, mod reports -> report captures live chat snapshot!
    send_msg(
        &mut target_ws,
        &ClientMessage::ChatSend {
            text: "rude chat line".to_owned(),
            scope: Some(ChatScopeDto::Lobby),
        },
    )
    .await;
    recv_until(&mut mod_ws, |m| match m {
        ServerMessage::Chat { line, .. } if line.text.contains("rude chat") => Some(()),
        _ => None,
    })
    .await;
    send_msg(
        &mut mod_ws,
        &ClientMessage::Report {
            user_id: target_id.clone(),
            reason: "harassment".to_owned(),
        },
    )
    .await;
    let report_id = recv_until(&mut mod_ws, |m| match m {
        ServerMessage::ReportSaved { id } => Some(id),
        _ => None,
    })
    .await;
    assert!(report_id >= 1);
    let saved_reports = srv.repos.reports();
    assert!(
        saved_reports
            .iter()
            .any(|(_, _, reason, log)| reason == "harassment"
                && log.as_deref().is_some_and(|l| l.contains("rude chat")))
    );

    // Moderator closes target's table -> target gets Kicked(TableClosed).
    send_msg(
        &mut target_ws,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let tid = recv_until(&mut target_ws, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s.id),
        _ => None,
    })
    .await;
    send_msg(&mut mod_ws, &ClientMessage::ModCloseTable { table_id: tid }).await;
    let kicked = recv_until(&mut target_ws, |m| match m {
        ServerMessage::Kicked { reason } => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(kicked, KickReasonDto::TableClosed);

    // Moderator bans target -> active socket closed & old session rejected with Banned!
    send_msg(
        &mut mod_ws,
        &ClientMessage::ModBan {
            user_id: target_id,
            hours: 24,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let (mut banned_retry, _) = connect_async(&srv.base_ws).await.unwrap();
    send_msg(
        &mut banned_retry,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: Some(target_session),
            guest: None,
        },
    )
    .await;
    let ban_err = recv_until(&mut banned_retry, |m| match m {
        ServerMessage::Error { code, .. } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(ban_err, ErrorCode::Banned);

    // Graceful drain broadcasts system notice & refuses new tables.
    srv.state.hub.begin_drain();
    let drain_notice = recv_until(&mut mod_ws, |m| match m {
        ServerMessage::Chat { line, .. } if line.sys && line.text.contains("Server update") => {
            Some(line.text)
        }
        _ => None,
    })
    .await;
    assert!(drain_notice.contains("running games will finish"));
    send_msg(
        &mut mod_ws,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let drain_err = recv_until(&mut mod_ws, |m| match m {
        ServerMessage::Error { code, re } if re == "table.create" => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(drain_err, ErrorCode::NotAllowed);
}

async fn spawn_load_client(ws_url: String, room: &'static str) -> WsStream {
    let (mut ws, _) = connect_async(&ws_url).await.unwrap();
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    recv_until(&mut ws, |m| {
        matches!(m, ServerMessage::Welcome { .. }).then_some(())
    })
    .await;
    send_msg(
        &mut ws,
        &ClientMessage::LobbyJoin {
            room: room.to_owned(),
        },
    )
    .await;
    let _ = recv_until(&mut ws, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    ws
}

#[tokio::test]
async fn load_test_200_concurrent_users_stay_healthy_and_auto_open_rooms() {
    let srv = spawn_server(Duration::from_secs(45)).await;

    // Wave 1: 160 concurrent users (80 Pool + 80 Checkers), crossing 70% threshold in both rooms.
    let mut wave1 = Vec::with_capacity(160);
    for idx in 0..160_usize {
        let room = if idx < 80 {
            "pool-social-1"
        } else {
            "checkers-social-1"
        };
        wave1.push(tokio::spawn(spawn_load_client(srv.base_ws.clone(), room)));
    }
    let mut open_sockets = Vec::with_capacity(200);
    for h in wave1 {
        open_sockets.push(h.await.unwrap());
    }

    // Both overflow rooms must now be open!
    let rooms = srv.state.hub.list_rooms();
    assert!(rooms.iter().any(|r| r.id == "pool-social-2"));
    assert!(rooms.iter().any(|r| r.id == "checkers-social-2"));

    // Wave 2: 40 more concurrent users join the auto-opened overflow rooms (200 total live sockets).
    let mut wave2 = Vec::with_capacity(40);
    for idx in 0..40_usize {
        let room = if idx < 20 {
            "pool-social-2"
        } else {
            "checkers-social-2"
        };
        wave2.push(tokio::spawn(spawn_load_client(srv.base_ws.clone(), room)));
    }
    for h in wave2 {
        open_sockets.push(h.await.unwrap());
    }
    assert_eq!(open_sockets.len(), 200);

    // `/status` reports healthy 200 OK with all 200 live users.
    let status: serde_json::Value = reqwest::get(format!("{}/status", srv.base_http))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["status"], "ok");
    assert_eq!(status["online"], 200);

    for mut ws in open_sockets {
        let _ = ws.close(None).await;
    }
}

#[tokio::test]
async fn graceful_shutdown_disconnects_idle_ws_clients_immediately() {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"graceful-shutdown-test-secret!!!"));
    let state = AppState::new(
        ServerConfig::default(),
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        mailer,
        google,
    );
    let drain_hub = Arc::clone(&state.hub);
    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{addr}/ws");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
                drain_hub.begin_drain();
                drain_hub.disconnect_all_clients();
            })
            .await
            .unwrap();
    });

    // 1. An authenticated client sitting idle in a lounge room.
    let mut idle_ws = spawn_load_client(ws_url.clone(), "pool-social-1").await;
    // 2. A raw WebSocket connected prior to completing Hello handshake.
    let (mut pre_hello_ws, _) = connect_async(&ws_url).await.unwrap();

    // Trigger graceful shutdown -> server must disconnect both clients and exit promptly!
    shutdown_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), serve_handle)
        .await
        .expect("axum::serve hung waiting on idle WebSocket clients")
        .unwrap();

    // Both clients observe clean closure (Close frame or EOF) quickly.
    let closed_idle = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(frame) = idle_ws.next().await {
            if matches!(frame, Ok(WsMsg::Close(_)) | Err(_)) {
                break;
            }
        }
    })
    .await;
    assert!(closed_idle.is_ok(), "idle client was not disconnected");

    let closed_pre = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(frame) = pre_hello_ws.next().await {
            if matches!(frame, Ok(WsMsg::Close(_)) | Err(_)) {
                break;
            }
        }
    })
    .await;
    assert!(closed_pre.is_ok(), "pre-hello client was not disconnected");
}

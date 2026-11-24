//! Multi-client TCP + WebSocket integration tests for Milestone 5 actors
//! (lounges, tables, chat, invites, `Play Now`, shareable table links, and backpressure).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;
use std::time::Duration;

use boomer_protocol::{
    BotLevelDto, ChatScopeDto, ClientMessage, ErrorCode, GameKindDto, KickReasonDto,
    PROTOCOL_VERSION, ServerMessage, TablePhaseDto, TableRoleDto, TableSettingsDto,
    TableSnapshotDto, TableVisibilityDto,
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
}

async fn spawn_server(room_capacity: usize, auto_open_threshold: u32) -> TestServer {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"milestone-5-integration-secret!!"));
    let state = AppState::new(
        ServerConfig::default(),
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos,
        mailer,
        google,
    )
    .with_room_limits(room_capacity, auto_open_threshold);

    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    TestServer {
        base_http: format!("http://{addr}"),
        base_ws: format!("ws://{addr}/ws"),
    }
}

async fn send_msg(ws: &mut WsStream, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(WsMsg::Text(text.into())).await.unwrap();
}

async fn recv_msg(ws: &mut WsStream) -> ServerMessage {
    let deadline = tokio::time::sleep(Duration::from_secs(3));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => panic!("timed out waiting for ServerMessage"),
            frame = ws.next() => {
                let frame = frame.expect("socket closed unexpectedly").unwrap();
                if let WsMsg::Text(text) = frame {
                    return serde_json::from_str(&text).unwrap();
                }
            }
        }
    }
}

async fn recv_until<F, T>(ws: &mut WsStream, mut pred: F) -> T
where
    F: FnMut(ServerMessage) -> Option<T>,
{
    let deadline = tokio::time::sleep(Duration::from_secs(3));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => panic!("timed out waiting for expected message"),
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

async fn recv_table_snapshot(ws: &mut WsStream) -> TableSnapshotDto {
    *recv_until(ws, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await
}

async fn join_table_snapshot(
    ws: &mut WsStream,
    table_id: &str,
    role: TableRoleDto,
) -> TableSnapshotDto {
    send_msg(
        ws,
        &ClientMessage::TableJoin {
            table_id: table_id.to_owned(),
            role,
        },
    )
    .await;
    recv_table_snapshot(ws).await
}

async fn expect_err(ws: &mut WsStream, re_name: &str) -> ErrorCode {
    recv_until(ws, |m| match m {
        ServerMessage::Error { code, re } if re == re_name => Some(code),
        _ => None,
    })
    .await
}

async fn connect_and_join_room(srv: &TestServer, room: &str) -> (WsStream, String) {
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
    let wire_id = match recv_msg(&mut ws).await {
        ServerMessage::Welcome { me: Some(me), .. } => me.id,
        other => panic!("expected Welcome, got {other:?}"),
    };
    send_msg(
        &mut ws,
        &ClientMessage::LobbyJoin {
            room: room.to_owned(),
        },
    )
    .await;
    let _snap = recv_until(&mut ws, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    (ws, wire_id)
}

#[tokio::test]
async fn room_join_auto_opens_overflow_room_and_filters_chat() {
    let srv = spawn_server(10, 2).await;

    let (mut client_a, id_a) = connect_and_join_room(&srv, "pool-social-1").await;
    let (mut client_b, id_b) = connect_and_join_room(&srv, "pool-social-1").await;

    let rooms = recv_until(&mut client_a, |m| match m {
        ServerMessage::RoomsUpdate { rooms } => Some(rooms),
        _ => None,
    })
    .await;
    assert!(rooms.iter().any(|r| r.id == "pool-social-2"));

    send_msg(
        &mut client_a,
        &ClientMessage::ChatSend {
            text: "What a FUCK shot!".to_owned(),
            scope: Some(ChatScopeDto::Lobby),
        },
    )
    .await;
    let chat_text = recv_until(&mut client_b, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        } if line.from.as_deref() == Some(id_a.as_str()) => Some(line.text),
        _ => None,
    })
    .await;
    assert_eq!(chat_text, "What a *** shot!");

    send_msg(
        &mut client_a,
        &ClientMessage::ChatSend {
            text: "second message too fast".to_owned(),
            scope: Some(ChatScopeDto::Lobby),
        },
    )
    .await;
    assert_eq!(
        expect_err(&mut client_a, "chat.send").await,
        ErrorCode::RateLimited
    );

    let _ = client_b.close(None).await;
    let removed_id = recv_until(&mut client_a, |m| match m {
        ServerMessage::UserRemove { id } => Some(id),
        _ => None,
    })
    .await;
    assert_eq!(removed_id, id_b);
}

#[tokio::test]
async fn table_host_controls_mute_spectators_and_boot_block() {
    let srv = spawn_server(100, 70).await;
    let (mut host, _) = connect_and_join_room(&srv, "pool-social-1").await;
    let (mut opp, _) = connect_and_join_room(&srv, "pool-social-1").await;
    let (mut watcher, watcher_id) = connect_and_join_room(&srv, "pool-social-1").await;

    send_msg(
        &mut host,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_table_snapshot(&mut host).await.id;
    let _ = join_table_snapshot(&mut opp, &table_id, TableRoleDto::Player).await;
    let _ = join_table_snapshot(&mut watcher, &table_id, TableRoleDto::Spectator).await;

    let mute_cfg = TableSettingsDto {
        mute_spectators: true,
        ..TableSettingsDto::default()
    };
    send_msg(&mut opp, &ClientMessage::SettingsSet(mute_cfg.clone())).await;
    assert_eq!(
        expect_err(&mut opp, "settings.set").await,
        ErrorCode::NotHost
    );

    send_msg(&mut host, &ClientMessage::SettingsSet(mute_cfg)).await;
    let _ = recv_until(&mut watcher, |m| match m {
        ServerMessage::TableUpdate(u) if u.settings.mute_spectators => Some(u),
        _ => None,
    })
    .await;
    send_msg(
        &mut watcher,
        &ClientMessage::ChatSend {
            text: "hi from gallery".to_owned(),
            scope: Some(ChatScopeDto::Table),
        },
    )
    .await;
    assert_eq!(
        expect_err(&mut watcher, "chat.send").await,
        ErrorCode::Muted
    );

    send_msg(
        &mut host,
        &ClientMessage::Boot {
            user_id: watcher_id,
        },
    )
    .await;
    let kick_reason = recv_until(&mut watcher, |m| match m {
        ServerMessage::Kicked { reason } => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(kick_reason, KickReasonDto::Booted);

    send_msg(
        &mut watcher,
        &ClientMessage::TableJoin {
            table_id,
            role: TableRoleDto::Spectator,
        },
    )
    .await;
    assert_eq!(
        expect_err(&mut watcher, "table.join").await,
        ErrorCode::NotAllowed
    );
}

#[tokio::test]
async fn table_stand_up_bot_add_ready_and_status_metrics() {
    let srv = spawn_server(100, 70).await;
    let (mut host, _) = connect_and_join_room(&srv, "pool-social-1").await;
    let (mut opp, _) = connect_and_join_room(&srv, "pool-social-1").await;

    send_msg(
        &mut host,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_table_snapshot(&mut host).await.id;
    let _ = join_table_snapshot(&mut opp, &table_id, TableRoleDto::Player).await;

    send_msg(&mut opp, &ClientMessage::SeatLeave {}).await;
    let _ = recv_until(&mut host, |m| match m {
        ServerMessage::TableUpdate(u) if u.seats[1].is_none() => Some(u),
        _ => None,
    })
    .await;

    send_msg(
        &mut host,
        &ClientMessage::BotAdd {
            level: BotLevelDto::Medium,
        },
    )
    .await;
    let bot_upd = recv_until(&mut host, |m| match m {
        ServerMessage::TableUpdate(u) if u.seats[1].as_ref().is_some_and(|s| s.bot) => Some(u),
        _ => None,
    })
    .await;
    assert!(!bot_upd.settings.rated);

    send_msg(&mut host, &ClientMessage::Ready {}).await;
    let started = recv_until(&mut host, |m| match m {
        ServerMessage::TableUpdate(u) if u.phase == TablePhaseDto::Playing => Some(u),
        _ => None,
    })
    .await;
    assert_eq!(started.phase, TablePhaseDto::Playing);

    let status: serde_json::Value = reqwest::get(format!("{}/status", srv.base_http))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["tables"], 1);
    assert_eq!(status["games"], 1);
}

#[tokio::test]
async fn invites_and_ignore_work_end_to_end() {
    let srv = spawn_server(100, 70).await;
    let (mut alice, alice_id) = connect_and_join_room(&srv, "checkers-social-1").await;
    let (mut bob, bob_id) = connect_and_join_room(&srv, "checkers-social-1").await;

    send_msg(
        &mut alice,
        &ClientMessage::InviteSend {
            to: bob_id.clone(),
            table_id: None,
            settings: Some(TableSettingsDto {
                visibility: TableVisibilityDto::Private,
                ..TableSettingsDto::default()
            }),
        },
    )
    .await;
    let invite_id = recv_until(&mut bob, |m| match m {
        ServerMessage::InviteReceived {
            invite_id,
            from,
            game,
            ..
        } => {
            assert_eq!(from.id, alice_id);
            assert_eq!(game, GameKindDto::Checkers);
            Some(invite_id)
        }
        _ => None,
    })
    .await;

    send_msg(
        &mut bob,
        &ClientMessage::InviteAnswer {
            invite_id,
            accept: true,
        },
    )
    .await;
    let alice_snap = recv_table_snapshot(&mut alice).await;
    let bob_snap = recv_table_snapshot(&mut bob).await;
    assert_eq!(alice_snap.id, bob_snap.id);
    assert_eq!(alice_snap.seats[0].as_ref().unwrap().id, alice_id);
    assert_eq!(alice_snap.seats[1].as_ref().unwrap().id, bob_id);

    send_msg(&mut alice, &ClientMessage::TableLeave {}).await;
    send_msg(&mut bob, &ClientMessage::TableLeave {}).await;
    send_msg(
        &mut bob,
        &ClientMessage::IgnoreSet {
            user_id: alice_id,
            on: true,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    send_msg(
        &mut alice,
        &ClientMessage::InviteSend {
            to: bob_id,
            table_id: None,
            settings: None,
        },
    )
    .await;
    assert_eq!(
        expect_err(&mut alice, "invite.send").await,
        ErrorCode::NotAllowed
    );
}

#[tokio::test]
async fn play_now_human_and_bot_matchmaking_work() {
    let srv = spawn_server(100, 70).await;

    let (mut p1, p1_id) = connect_and_join_room(&srv, "pool-social-1").await;
    let (mut p2, p2_id) = connect_and_join_room(&srv, "pool-social-1").await;
    send_msg(&mut p1, &ClientMessage::PlayNow { vs_bot: None }).await;
    recv_until(&mut p1, |m| match m {
        ServerMessage::PlayNowQueued { .. } => Some(()),
        _ => None,
    })
    .await;
    send_msg(&mut p2, &ClientMessage::PlayNow { vs_bot: None }).await;

    let s1 = recv_table_snapshot(&mut p1).await;
    let s2 = recv_table_snapshot(&mut p2).await;
    assert_eq!(s1.id, s2.id);
    assert_eq!(s1.seats[0].as_ref().unwrap().id, p1_id);
    assert_eq!(s1.seats[1].as_ref().unwrap().id, p2_id);

    let (mut solo, _) = connect_and_join_room(&srv, "pool-social-1").await;
    send_msg(
        &mut solo,
        &ClientMessage::PlayNow {
            vs_bot: Some(BotLevelDto::Hard),
        },
    )
    .await;
    let computer_snap = recv_table_snapshot(&mut solo).await;
    assert_eq!(computer_snap.seats[1].as_ref().unwrap().id, "bot3");
}

#[tokio::test]
async fn direct_table_link_joins_room_and_table_automatically() {
    let srv = spawn_server(100, 70).await;
    let (mut host, _) = connect_and_join_room(&srv, "checkers-social-1").await;
    send_msg(
        &mut host,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_table_snapshot(&mut host).await.id;

    let (mut link_guest, _) = connect_async(&srv.base_ws).await.unwrap();
    send_msg(
        &mut link_guest,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    let _ = recv_msg(&mut link_guest).await;
    let joined = join_table_snapshot(&mut link_guest, &table_id, TableRoleDto::Player).await;
    assert_eq!(joined.id, table_id);
    assert!(joined.seats[1].is_some());
}

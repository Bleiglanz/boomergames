//! WebSocket integration tests for the optional Claude AI Friend chat companion.
//!
//! Uses a deterministic in-memory [`FakeAiFriend`] port so zero Anthropic API tokens are spent.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use boomer_protocol::{
    ChatScopeDto, ClientMessage, ErrorCode, LobbySnapshotDto, PROTOCOL_VERSION, ServerMessage,
    TableRoleDto, TableSettingsDto, TableSnapshotDto,
};
use boomer_server::adapters::memory::{CaptureMailSender, MemoryRepos, TestGoogleVerifier};
use boomer_server::auth::TokenSigner;
use boomer_server::ports::{AiChatContext, AiFriend, BoxFuture, PortError};
use boomer_server::{AppState, ServerConfig, build_router};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct FakeAiFriend {
    calls: Mutex<Vec<AiChatContext>>,
    next_result: Mutex<Result<String, PortError>>,
}

impl FakeAiFriend {
    fn ok(reply: &str) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            next_result: Mutex::new(Ok(reply.to_owned())),
        }
    }

    fn budget_exhausted() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            next_result: Mutex::new(Err(PortError::Conflict("ai_budget_exhausted".to_owned()))),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn last_context(&self) -> Option<AiChatContext> {
        self.calls.lock().unwrap().last().cloned()
    }
}

impl AiFriend for FakeAiFriend {
    fn is_configured(&self) -> bool {
        true
    }

    fn generate_reply(&self, ctx: AiChatContext) -> BoxFuture<'_, Result<String, PortError>> {
        self.calls.lock().unwrap().push(ctx);
        let res = self.next_result.lock().unwrap().clone();
        Box::pin(async move { res })
    }
}

async fn spawn_server(ai: Option<Arc<dyn AiFriend>>) -> String {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"ai-friend-integration-test-key!!"));
    let mut state = AppState::new(
        ServerConfig::default(),
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos,
        mailer,
        google,
    );
    if let Some(ai_friend) = ai {
        state = state.with_ai_friend(ai_friend);
    }
    state.hub.set_ai_reply_cooldown(Duration::ZERO);

    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("ws://{addr}/ws")
}

async fn send_msg(ws: &mut WsStream, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(WsMsg::Text(text.into())).await.unwrap();
}

async fn send_chat(ws: &mut WsStream, scope: ChatScopeDto, text: &str) {
    send_msg(
        ws,
        &ClientMessage::ChatSend {
            text: text.to_owned(),
            scope: Some(scope),
        },
    )
    .await;
}

async fn recv_until<F, T>(ws: &mut WsStream, mut pred: F) -> T
where
    F: FnMut(ServerMessage) -> Option<T>,
{
    let deadline = tokio::time::sleep(Duration::from_secs(3));
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

async fn connect_and_join_lobby(ws_url: &str, room: &str) -> (WsStream, LobbySnapshotDto) {
    let (mut ws, _) = connect_async(ws_url).await.unwrap();
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
    let snap = *recv_until(&mut ws, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    (ws, snap)
}

#[tokio::test]
async fn lounge_defaults_on_replies_after_two_messages_and_respects_toggle_off() {
    let fake_ai = Arc::new(FakeAiFriend::ok(
        "Why did the pool player bring a ladder? To reach the high shots! Have fun!",
    ));
    let ws_url = spawn_server(Some(fake_ai.clone())).await;

    let (mut c1, snap1) = connect_and_join_lobby(&ws_url, "pool-social-1").await;
    let (mut c2, snap2) = connect_and_join_lobby(&ws_url, "pool-social-1").await;

    assert!(snap1.ai_friend_available);
    assert!(
        snap1.ai_friend,
        "lounge chat should default to AI Friend ON"
    );
    assert!(snap2.ai_friend);

    // 1st message: no AI reply yet.
    send_chat(
        &mut c1,
        ChatScopeDto::Lobby,
        "Hey everyone, ready for some pool?",
    )
    .await;
    recv_until(&mut c2, |m| match m {
        ServerMessage::Chat { line, .. } if line.text.contains("ready for some pool") => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(fake_ai.call_count(), 0);

    // 2nd message: Claude chimes in!
    send_chat(&mut c2, ChatScopeDto::Lobby, "Always! Good luck tonight!").await;
    let ai_line = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        } if line.ai => Some(line),
        _ => None,
    })
    .await;
    assert_eq!(ai_line.name.as_deref(), Some("🤖 Claude (AI)"));
    assert!(ai_line.text.contains("ladder"));
    assert_eq!(fake_ai.call_count(), 1);
    let ctx = fake_ai.last_context().unwrap();
    assert!(ctx.setting.contains("8-Ball Pool Social Lounge"));
    assert_eq!(ctx.recent_messages.len(), 2);

    // Any player toggles AI Friend OFF -> state broadcast + chat notice.
    send_msg(
        &mut c2,
        &ClientMessage::ChatAiSet {
            scope: ChatScopeDto::Lobby,
            enabled: false,
        },
    )
    .await;
    let off_state = recv_until(&mut c1, |m| match m {
        ServerMessage::ChatAiState {
            scope: ChatScopeDto::Lobby,
            enabled,
        } => Some(enabled),
        _ => None,
    })
    .await;
    assert!(!off_state);
    let notice = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        } if line.sys && line.text.contains("turned AI Friend off") => Some(line.text),
        _ => None,
    })
    .await;
    assert!(notice.contains(&snap2.me.name));

    // Wait 1.1s for per-user chat rate limit, then send 2 more messages while OFF -> zero new AI calls.
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    send_chat(&mut c1, ChatScopeDto::Lobby, "Testing silence 1").await;
    send_chat(&mut c2, ChatScopeDto::Lobby, "Testing silence 2").await;
    recv_until(&mut c1, |m| match m {
        ServerMessage::Chat { line, .. } if line.text == "Testing silence 2" => Some(()),
        _ => None,
    })
    .await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(fake_ai.call_count(), 1);
}

#[tokio::test]
async fn table_defaults_off_and_tracks_chat_independently_from_lounge() {
    let fake_ai = Arc::new(FakeAiFriend::ok(
        "Checkmate? More like check-friend! You two are playing great!",
    ));
    let ws_url = spawn_server(Some(fake_ai.clone())).await;

    let (mut c1, _) = connect_and_join_lobby(&ws_url, "chess-social-1").await;
    let (mut c2, snap2) = connect_and_join_lobby(&ws_url, "chess-social-1").await;

    // Host creates table; tables default to AI Friend OFF even though lounge is ON.
    send_msg(
        &mut c1,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_snap: TableSnapshotDto = *recv_until(&mut c1, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    assert!(table_snap.ai_friend_available);
    assert!(!table_snap.ai_friend, "table chat should default to OFF");

    // Second player joins table.
    send_msg(
        &mut c2,
        &ClientMessage::TableJoin {
            table_id: table_snap.id.clone(),
            role: TableRoleDto::Player,
        },
    )
    .await;
    recv_until(&mut c2, |m| {
        matches!(m, ServerMessage::TableSnapshot(_)).then_some(())
    })
    .await;

    // Two table messages while OFF -> 0 AI calls.
    send_chat(&mut c1, ChatScopeDto::Table, "Quiet table chat 1").await;
    send_chat(&mut c2, ChatScopeDto::Table, "Quiet table chat 2").await;
    recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        } if line.text == "Quiet table chat 2" => Some(()),
        _ => None,
    })
    .await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(fake_ai.call_count(), 0);

    // Non-host player (c2) toggles AI Friend ON for the table -> state + announcement.
    send_msg(
        &mut c2,
        &ClientMessage::ChatAiSet {
            scope: ChatScopeDto::Table,
            enabled: true,
        },
    )
    .await;
    assert!(
        recv_until(&mut c1, |m| match m {
            ServerMessage::ChatAiState {
                scope: ChatScopeDto::Table,
                enabled,
            } => Some(enabled),
            _ => None,
        })
        .await
    );
    let notice = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        } if line.sys && line.text.contains("turned AI Friend on") => Some(line.text),
        _ => None,
    })
    .await;
    assert!(notice.contains(&snap2.me.name));

    // Wait for 1s chat rate limit, then 2 table messages -> Claude replies to table scope.
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    send_chat(&mut c1, ChatScopeDto::Table, "Good luck in chess!").await;
    send_chat(&mut c2, ChatScopeDto::Table, "Thanks, have fun!").await;
    let ai_line = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        } if line.ai => Some(line),
        _ => None,
    })
    .await;
    assert!(ai_line.text.contains("check-friend"));
    let ctx = fake_ai.last_context().unwrap();
    assert!(ctx.setting.contains("Chess"));
    assert!(ctx.setting.contains(&format!("Table #{}", table_snap.no)));
}

#[tokio::test]
async fn budget_exhaustion_disables_ai_friend_with_polite_notice() {
    let fake_ai = Arc::new(FakeAiFriend::budget_exhausted());
    let ws_url = spawn_server(Some(fake_ai)).await;

    let (mut c1, _) = connect_and_join_lobby(&ws_url, "checkers-social-1").await;
    let (mut c2, _) = connect_and_join_lobby(&ws_url, "checkers-social-1").await;

    send_chat(&mut c1, ChatScopeDto::Lobby, "Hello checkers!").await;
    send_chat(&mut c2, ChatScopeDto::Lobby, "Hi there!").await;

    let disabled = recv_until(&mut c1, |m| match m {
        ServerMessage::ChatAiState {
            scope: ChatScopeDto::Lobby,
            enabled,
        } => Some(enabled),
        _ => None,
    })
    .await;
    assert!(!disabled);

    let notice = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        } if line.sys && line.text.contains("API call limit reached") => Some(line.text),
        _ => None,
    })
    .await;
    assert!(notice.contains("taking a break"));
}

#[tokio::test]
async fn solo_player_gets_immediate_ai_reply_in_lounge_and_table() {
    let fake_ai = Arc::new(FakeAiFriend::ok(
        "Welcome! Click Play Now or join a House Bot table anytime — why did the chess piece go to bed? It was a knight!",
    ));
    let ws_url = spawn_server(Some(fake_ai.clone())).await;

    // Solo player in Social Lounge gets an immediate reply on their 1st message.
    let (mut c1, snap1) = connect_and_join_lobby(&ws_url, "pool-social-1").await;
    assert!(snap1.ai_friend);
    send_chat(&mut c1, ChatScopeDto::Lobby, "Hello? Anyone here?").await;
    let ai_lobby = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        } if line.ai => Some(line),
        _ => None,
    })
    .await;
    assert!(ai_lobby.text.contains("Play Now"));
    assert_eq!(fake_ai.call_count(), 1);

    // Solo player creates a table, enables AI Friend, and gets an immediate reply on 1st message.
    send_msg(
        &mut c1,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let _table = *recv_until(&mut c1, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    send_msg(
        &mut c1,
        &ClientMessage::ChatAiSet {
            scope: ChatScopeDto::Table,
            enabled: true,
        },
    )
    .await;
    recv_until(&mut c1, |m| {
        matches!(
            m,
            ServerMessage::ChatAiState {
                scope: ChatScopeDto::Table,
                enabled: true
            }
        )
        .then_some(())
    })
    .await;

    tokio::time::sleep(Duration::from_millis(1_100)).await;
    send_chat(&mut c1, ChatScopeDto::Table, "How do I start a game?").await;
    let ai_table = recv_until(&mut c1, |m| match m {
        ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        } if line.ai => Some(line),
        _ => None,
    })
    .await;
    assert!(ai_table.text.contains("Welcome!"));
    assert_eq!(fake_ai.call_count(), 2);
}

#[tokio::test]
async fn unconfigured_server_reports_unavailable_and_rejects_toggle() {
    let ws_url = spawn_server(None).await;
    let (mut c1, snap) = connect_and_join_lobby(&ws_url, "pool-social-1").await;
    assert!(!snap.ai_friend_available);
    assert!(!snap.ai_friend);

    send_msg(
        &mut c1,
        &ClientMessage::ChatAiSet {
            scope: ChatScopeDto::Lobby,
            enabled: true,
        },
    )
    .await;
    let code = recv_until(&mut c1, |m| match m {
        ServerMessage::Error { code, re } if re == "chat.ai.set" => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(code, ErrorCode::NotAllowed);
}

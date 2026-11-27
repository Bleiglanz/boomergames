//! WebSocket connection handler enforcing handshake, 4 KB frame cap, rate limits,
//! 20s ping/pong liveness, non-blocking outbound queues, auth, and actor routing.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use boomer_protocol::{
    ChatScopeDto, ClientMessage, ErrorCode, GameKindDto, PROTOCOL_VERSION, ServerMessage,
    SessionKindDto, TableRoleDto, UserDto, UserStatusDto,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Notify, mpsc, oneshot};
use tracing::{debug, warn};

use crate::actors::ClientSink;
use crate::actors::lobby_actor::LobbyCmd;
use crate::actors::table_actor::{TableClient, TableCmd};
use crate::auth::validate_email;
use crate::mapping::{
    game_kind_from_dto, guest_to_user_dto, highscore_entry_to_dto, match_item_to_dto,
    parse_user_id, stat_record_to_profile_dto, user_record_to_dto,
};
use crate::ports::{PortError, UserRecord};
use crate::state::AppState;

/// Maximum inbound JSON text frame size (4 KB per `incubation/plan.md` §9.6).
const MAX_MESSAGE_BYTES: usize = 4_096;

/// Bounded per-client outbound buffer; slow clients overflowing it are disconnected cleanly.
const OUTBOUND_QUEUE_CAP: usize = 256;

/// Server keepalive ping interval (`20 s`).
const PING_INTERVAL: Duration = Duration::from_secs(20);

/// Disconnect after `2` missed pongs (40 s silent socket).
const MAX_MISSED_PONGS: u8 = 2;

/// General rate limit: max `10` messages per second.
const MAX_MSGS_PER_SEC: u32 = 10;

/// Repeated rate-limit strikes before dropping an abusive socket.
const MAX_RATE_STRIKES: u8 = 5;

#[derive(Debug, Clone)]
enum ConnIdentity {
    Guest,
    Registered { user_id: i64 },
}

struct ConnSession {
    identity: ConnIdentity,
    user: UserDto,
    muted: bool,
    ignored: HashSet<String>,
    active_room: Option<String>,
    out_tx: mpsc::Sender<Message>,
    sink: ClientSink,
    slow_disconnect: Arc<AtomicBool>,
    disconnect_notify: Arc<Notify>,
}

impl ConnSession {
    fn send_msg(&self, msg: &ServerMessage) -> bool {
        self.sink.send_msg(msg)
    }

    fn send_error(&self, code: ErrorCode, re: &str) -> bool {
        self.send_msg(&ServerMessage::Error {
            code,
            re: re.to_owned(),
        })
    }

    fn to_table_client(&self) -> TableClient {
        TableClient {
            user: self.user.clone(),
            sink: self.sink.clone(),
            last_chat_at: None,
            muted: self.muted,
        }
    }
}

struct RateLimiter {
    window_start: Instant,
    count: u32,
    strikes: u8,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
            strikes: 0,
        }
    }

    fn allow(&mut self) -> Result<(), bool> {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.count = 0;
        }
        self.count += 1;
        if self.count > MAX_MSGS_PER_SEC {
            self.strikes = self.strikes.saturating_add(1);
            return Err(self.strikes >= MAX_RATE_STRIKES);
        }
        Ok(())
    }
}

/// `GET /ws` upgrade endpoint.
pub async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE_BYTES * 2)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    state.online.fetch_add(1, Ordering::Relaxed);
    let _guard = OnlineGuard {
        state: state.clone(),
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(OUTBOUND_QUEUE_CAP);
    let slow_disconnect = Arc::new(AtomicBool::new(false));
    let disconnect_notify = Arc::new(Notify::new());

    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if ws_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    if state.hub.is_shutting_down() {
        let _ = out_tx.try_send(Message::Close(None));
        drop(out_tx);
        let _ = writer.await;
        return;
    }

    let Some(mut session) = perform_handshake(
        &mut ws_rx,
        out_tx.clone(),
        Arc::clone(&slow_disconnect),
        Arc::clone(&disconnect_notify),
        &state,
    )
    .await
    else {
        let _ = out_tx.try_send(Message::Close(None));
        drop(out_tx);
        let _ = writer.await;
        return;
    };

    state.hub.register_connection(session.sink.clone());
    if state.hub.is_shutting_down() {
        session.sink.force_disconnect();
    } else {
        attempt_table_reconnect(&mut session, &state).await;
        run_message_loop(&mut ws_rx, &mut session, &state).await;
    }
    cleanup_session(&session, &state, true);

    let _ = out_tx.try_send(Message::Close(None));
    drop(out_tx);
    let _ = writer.await;
}

async fn attempt_table_reconnect(session: &mut ConnSession, state: &AppState) {
    let Some(tid) = state.hub.user_table(&session.user.id) else {
        return;
    };
    let Some(table) = state.hub.find_table(&tid) else {
        return;
    };
    let _ = join_lobby_room(&table.room_id, false, session, state).await;
    let (tx, rx) = oneshot::channel();
    if table
        .send(TableCmd::Reconnect {
            user_id: session.user.id.clone(),
            sink: session.sink.clone(),
            reply: tx,
        })
        .await
        && let Ok(Some((table_snap, game_snap))) = rx.await
    {
        let _ = session.send_msg(&ServerMessage::TableSnapshot(Box::new(table_snap)));
        if let Some(gs) = game_snap {
            let _ = session.send_msg(&gs);
        }
    }
}

async fn run_message_loop(
    ws_rx: &mut futures_util::stream::SplitStream<WebSocket>,
    session: &mut ConnSession,
    state: &AppState,
) {
    let mut ping_ticker = tokio::time::interval(PING_INTERVAL);
    ping_ticker.tick().await;
    let mut missed_pongs: u8 = 0;
    let mut limiter = RateLimiter::new();

    loop {
        if session.slow_disconnect.load(Ordering::Acquire) {
            debug!(user = %session.user.id, "disconnecting slow client");
            break;
        }
        tokio::select! {
            () = session.disconnect_notify.notified() => {
                if session.slow_disconnect.load(Ordering::Acquire) {
                    debug!(user = %session.user.id, "disconnecting slow client on notify");
                    break;
                }
            }
            _ = ping_ticker.tick() => {
                if missed_pongs >= MAX_MISSED_PONGS {
                    debug!("closing silent WebSocket after missed pongs");
                    break;
                }
                missed_pongs = missed_pongs.saturating_add(1);
                if session.out_tx.try_send(Message::Ping(Vec::new().into())).is_err() {
                    debug!("disconnecting slow WebSocket client on ping backpressure");
                    break;
                }
            }
            maybe_frame = ws_rx.next() => {
                let Some(Ok(frame)) = maybe_frame else {
                    break;
                };
                if !handle_ws_frame(frame, &mut missed_pongs, &mut limiter, session, state).await {
                    break;
                }
            }
        }
    }
}

async fn handle_ws_frame(
    frame: Message,
    missed_pongs: &mut u8,
    limiter: &mut RateLimiter,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    match frame {
        Message::Pong(_) => {
            *missed_pongs = 0;
            true
        }
        Message::Ping(payload) => session.out_tx.try_send(Message::Pong(payload)).is_ok(),
        Message::Close(_) => false,
        Message::Binary(_) => session.send_error(ErrorCode::Invalid, "binary"),
        Message::Text(text) => {
            if text.len() > MAX_MESSAGE_BYTES {
                return session.send_error(ErrorCode::Invalid, "frame");
            }
            if let Err(should_disconnect) = limiter.allow() {
                let _ = session.send_error(ErrorCode::RateLimited, "rate");
                return !should_disconnect;
            }
            let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) else {
                return session.send_error(ErrorCode::Invalid, "parse");
            };
            handle_client_message(client_msg, session, state).await
        }
    }
}

fn cleanup_session(session: &ConnSession, state: &AppState, is_disconnect: bool) {
    if let Some(tid) = state.hub.user_table(&session.user.id)
        && let Some(table) = state.hub.find_table(&tid)
    {
        if is_disconnect {
            table.try_send(TableCmd::Disconnect {
                user_id: session.user.id.clone(),
            });
        } else {
            table.try_send(TableCmd::Leave {
                user_id: session.user.id.clone(),
            });
        }
    }
    if let Some(ref rid) = session.active_room
        && let Some(room) = state.hub.find_room(rid)
    {
        room.try_send(LobbyCmd::Leave {
            user_id: session.user.id.clone(),
        });
    }
    state.hub.unregister_connection(&session.user.id);
}

async fn perform_handshake(
    ws_rx: &mut futures_util::stream::SplitStream<WebSocket>,
    out_tx: mpsc::Sender<Message>,
    slow_disconnect: Arc<AtomicBool>,
    disconnect_notify: Arc<Notify>,
    state: &AppState,
) -> Option<ConnSession> {
    let first = tokio::select! {
        () = state.hub.shutdown_notified() => return None,
        res = tokio::time::timeout(Duration::from_secs(10), ws_rx.next()) => res.ok()??.ok()?,
    };
    let Message::Text(text) = first else {
        return None;
    };
    if text.len() > MAX_MESSAGE_BYTES {
        let _ = send_raw_error(&out_tx, ErrorCode::Invalid, "hello");
        return None;
    }
    let Ok(ClientMessage::Hello { v, session, guest }) =
        serde_json::from_str::<ClientMessage>(&text)
    else {
        let _ = send_raw_error(&out_tx, ErrorCode::NotAllowed, "hello");
        return None;
    };

    if v != PROTOCOL_VERSION {
        let _ = send_raw_error(&out_tx, ErrorCode::UpgradeRequired, "hello");
        return None;
    }

    // Try resuming a signed registered session token.
    if let Some(ref token) = session
        && let Ok(claims) = state.signer.verify_session(token)
        && let Ok(Some(user)) = state.users.find_by_id(claims.user_id).await
    {
        if user.banned_until.is_some_and(|until| until > Utc::now()) {
            let _ = send_raw_error(&out_tx, ErrorCode::Banned, "hello");
            return None;
        }
        if user.session_version == claims.session_version {
            let me = user_record_to_dto(&user, 1200, 0, UserStatusDto::Available);
            let sink = ClientSink::new(
                me.id.clone(),
                out_tx.clone(),
                Arc::clone(&slow_disconnect),
                Arc::clone(&disconnect_notify),
            );
            if !sink.send_msg(&ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                mode: SessionKindDto::Remote,
                me: Some(me.clone()),
                guest: None,
                rooms: state.default_rooms(),
            }) {
                return None;
            }
            return Some(ConnSession {
                identity: ConnIdentity::Registered { user_id: user.id },
                user: me,
                muted: is_muted(&user),
                ignored: ignored_wire_set(&user),
                active_room: None,
                out_tx,
                sink,
                slow_disconnect,
                disconnect_notify,
            });
        }
    }

    // Otherwise resume a valid guest reconnect token or issue a fresh guest identity.
    let (guest_num, issued_guest_token) = if let Some(ref g_tok) = guest
        && let Ok(claims) = state.signer.verify_guest(g_tok)
    {
        (claims.guest_num, None)
    } else {
        let num = state.allocate_guest_num();
        let tok = state.signer.sign_guest(num).ok();
        (num, tok)
    };

    let me = guest_to_user_dto(guest_num, UserStatusDto::Available);
    let sink = ClientSink::new(
        me.id.clone(),
        out_tx.clone(),
        Arc::clone(&slow_disconnect),
        Arc::clone(&disconnect_notify),
    );
    if !sink.send_msg(&ServerMessage::Welcome {
        protocol_version: PROTOCOL_VERSION,
        mode: SessionKindDto::Remote,
        me: Some(me.clone()),
        guest: issued_guest_token,
        rooms: state.default_rooms(),
    }) {
        return None;
    }
    Some(ConnSession {
        identity: ConnIdentity::Guest,
        user: me,
        muted: false,
        ignored: HashSet::new(),
        active_room: None,
        out_tx,
        sink,
        slow_disconnect,
        disconnect_notify,
    })
}

async fn handle_client_message(
    msg: ClientMessage,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    match msg {
        ClientMessage::AuthEmail { email } => handle_auth_email(&email, session, state).await,
        ClientMessage::AuthMagic { token } => handle_auth_magic(&token, session, state).await,
        ClientMessage::AuthGoogle { id_token } => {
            handle_auth_google(&id_token, session, state).await
        }
        ClientMessage::AuthLogout {} => handle_auth_logout(session, state).await,
        ClientMessage::AccountUpdate { name, avatar } => {
            handle_account_update(name.as_deref(), avatar, session, state).await
        }
        ClientMessage::AccountDelete {} => handle_account_delete(session, state).await,
        ClientMessage::ProfileGet { user_id } => handle_profile_get(&user_id, session, state).await,
        ClientMessage::HighscoresGet { game, around } => {
            handle_highscores_get(game, around, session, state).await
        }
        ClientMessage::LobbyJoin { room } => handle_lobby_join(&room, session, state).await,
        ClientMessage::ChatSend { text, scope } => handle_chat_send(text, scope, session, state),
        ClientMessage::ChatAiSet { scope, enabled } => {
            handle_chat_ai_set(scope, enabled, session, state)
        }
        ClientMessage::TableCreate(settings) => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "table.create");
            };
            let _ = room
                .send(LobbyCmd::CreateTable {
                    user_id: session.user.id.clone(),
                    settings,
                })
                .await;
            true
        }
        ClientMessage::TableJoin { table_id, role } => {
            handle_table_join(&table_id, role, session, state).await
        }
        ClientMessage::TableLeave {} => {
            if let Some(table) = active_table_handle(session, state) {
                let _ = table
                    .send(TableCmd::Leave {
                        user_id: session.user.id.clone(),
                    })
                    .await;
            }
            true
        }
        ClientMessage::PlayNow { vs_bot } => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "play.now");
            };
            let _ = room
                .send(LobbyCmd::PlayNow {
                    user_id: session.user.id.clone(),
                    vs_bot,
                })
                .await;
            true
        }
        ClientMessage::InviteSend {
            to,
            table_id,
            settings,
        } => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "invite.send");
            };
            let _ = room
                .send(LobbyCmd::SendInvite {
                    from_id: session.user.id.clone(),
                    to_id: to,
                    table_id,
                    settings,
                })
                .await;
            true
        }
        ClientMessage::InviteAnswer { invite_id, accept } => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "invite.answer");
            };
            let _ = room
                .send(LobbyCmd::AnswerInvite {
                    user_id: session.user.id.clone(),
                    invite_id,
                    accept,
                })
                .await;
            true
        }
        ClientMessage::IgnoreSet { user_id, on } => {
            handle_ignore_set(&user_id, on, session, state).await
        }
        other => handle_table_scoped_message(other, session, state).await,
    }
}

async fn handle_table_scoped_message(
    msg: ClientMessage,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    let uid = session.user.id.clone();
    match msg {
        ClientMessage::Ready {} => {
            send_table_cmd(
                session,
                state,
                "ready",
                TableCmd::ToggleReady { user_id: uid },
            )
            .await
        }
        ClientMessage::SettingsSet(settings) => {
            send_table_cmd(
                session,
                state,
                "settings.set",
                TableCmd::UpdateSettings {
                    user_id: uid,
                    settings,
                },
            )
            .await
        }
        ClientMessage::SeatTake { seat } => {
            send_table_cmd(
                session,
                state,
                "seat.take",
                TableCmd::TakeSeat { user_id: uid, seat },
            )
            .await
        }
        ClientMessage::SeatLeave {} => {
            send_table_cmd(
                session,
                state,
                "seat.leave",
                TableCmd::LeaveSeat { user_id: uid },
            )
            .await
        }
        ClientMessage::Boot { user_id } => {
            send_table_cmd(
                session,
                state,
                "boot",
                TableCmd::BootUser {
                    host_id: uid,
                    target_id: user_id,
                },
            )
            .await
        }
        ClientMessage::BotAdd { level } => {
            send_table_cmd(
                session,
                state,
                "bot.add",
                TableCmd::AddBot {
                    host_id: uid,
                    level,
                },
            )
            .await
        }
        ClientMessage::BotRemove {} => {
            send_table_cmd(
                session,
                state,
                "bot.remove",
                TableCmd::RemoveBot { host_id: uid },
            )
            .await
        }
        other => handle_game_action_message(other, uid, session, state).await,
    }
}

async fn handle_game_action_message(
    msg: ClientMessage,
    uid: String,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    match msg {
        ClientMessage::CheckersMove { turn, path } => {
            send_table_cmd(
                session,
                state,
                "checkers.move",
                TableCmd::CheckersMove {
                    user_id: uid,
                    turn,
                    path,
                },
            )
            .await
        }
        ClientMessage::ChessMove { turn, uci } => {
            send_table_cmd(
                session,
                state,
                "chess.move",
                TableCmd::ChessMove {
                    user_id: uid,
                    turn,
                    uci,
                },
            )
            .await
        }
        ClientMessage::PoolShoot { shot } => {
            send_table_cmd(
                session,
                state,
                "pool.shoot",
                TableCmd::PoolShoot { user_id: uid, shot },
            )
            .await
        }
        ClientMessage::PoolPlaceCue { x, y } => {
            send_table_cmd(
                session,
                state,
                "pool.place_cue",
                TableCmd::PoolPlaceCue { user_id: uid, x, y },
            )
            .await
        }
        ClientMessage::Resign {} => {
            send_table_cmd(session, state, "resign", TableCmd::Resign { user_id: uid }).await
        }
        ClientMessage::DrawOffer {} => {
            send_table_cmd(
                session,
                state,
                "draw.offer",
                TableCmd::DrawOffer { user_id: uid },
            )
            .await
        }
        ClientMessage::DrawAnswer { accept } => {
            send_table_cmd(
                session,
                state,
                "draw.answer",
                TableCmd::DrawAnswer {
                    user_id: uid,
                    accept,
                },
            )
            .await
        }
        ClientMessage::Rematch {} => {
            send_table_cmd(
                session,
                state,
                "rematch",
                TableCmd::Rematch { user_id: uid },
            )
            .await
        }
        ClientMessage::Report { user_id, reason } => {
            handle_report(&user_id, &reason, session, state).await
        }
        ClientMessage::ModMute { user_id, minutes } => {
            handle_mod_mute(&user_id, minutes, session, state).await
        }
        ClientMessage::ModBan { user_id, hours } => {
            handle_mod_ban(&user_id, hours, session, state).await
        }
        ClientMessage::ModCloseTable { table_id } => {
            handle_mod_close_table(&table_id, session, state).await
        }
        ClientMessage::Hello { .. } => session.send_error(ErrorCode::NotAllowed, "hello"),
        _ => session.send_error(ErrorCode::NotAllowed, "unsupported"),
    }
}

async fn handle_report(
    target_wire_id: &str,
    reason: &str,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    let trimmed = reason.trim();
    if trimmed.is_empty() || trimmed.len() > 300 {
        return session.send_error(ErrorCode::Invalid, "report");
    }
    let reporter_id = match session.identity {
        ConnIdentity::Registered { user_id } => Some(user_id),
        ConnIdentity::Guest => None,
    };
    let target_id = parse_user_id(target_wire_id);
    let chat_snapshot = capture_recent_chat(session, state).await;
    match state
        .reports
        .create_report(
            reporter_id,
            target_id,
            target_wire_id,
            trimmed,
            chat_snapshot.as_deref(),
        )
        .await
    {
        Ok(id) => session.send_msg(&ServerMessage::ReportSaved { id }),
        Err(_) => session.send_error(ErrorCode::Invalid, "report"),
    }
}

async fn capture_recent_chat(session: &ConnSession, state: &AppState) -> Option<String> {
    if let Some(table) = active_table_handle(session, state) {
        let (tx, rx) = oneshot::channel();
        if table.send(TableCmd::DumpChat { reply: tx }).await
            && let Ok(log) = rx.await
            && !log.is_empty()
        {
            return Some(log);
        }
    }
    if let Some(room) = active_room_handle(session, state) {
        let (tx, rx) = oneshot::channel();
        if room.send(LobbyCmd::DumpChat { reply: tx }).await
            && let Ok(log) = rx.await
            && !log.is_empty()
        {
            return Some(log);
        }
    }
    None
}

async fn is_caller_moderator(session: &ConnSession, state: &AppState) -> bool {
    let ConnIdentity::Registered { user_id } = session.identity else {
        return false;
    };
    state
        .users
        .find_by_id(user_id)
        .await
        .ok()
        .flatten()
        .is_some_and(|u| u.is_moderator)
}

async fn handle_mod_mute(
    target_wire_id: &str,
    minutes: u32,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    if !is_caller_moderator(session, state).await {
        return session.send_error(ErrorCode::NotAllowed, "mod.mute");
    }
    let Some(target_uid) = parse_user_id(target_wire_id) else {
        return session.send_error(ErrorCode::Invalid, "mod.mute");
    };
    let until = if minutes == 0 {
        None
    } else {
        Some(Utc::now() + chrono::Duration::minutes(i64::from(minutes)))
    };
    if state.users.set_mute(target_uid, until).await.is_err() {
        return session.send_error(ErrorCode::Invalid, "mod.mute");
    }
    if let Ok(Some(target_user)) = state.users.find_by_id(target_uid).await {
        let dto = user_record_to_dto(&target_user, 1200, 0, UserStatusDto::Available);
        let muted_now = until.is_some();
        if let Some(room) = active_room_handle(session, state) {
            room.try_send(LobbyCmd::UpdateUser {
                user: dto.clone(),
                ignored: ignored_wire_set(&target_user),
                muted: muted_now,
            });
        }
        if let Some(tid) = state.hub.user_table(target_wire_id)
            && let Some(table) = state.hub.find_table(&tid)
        {
            table.try_send(TableCmd::UpdateMemberUser {
                user: dto,
                muted: muted_now,
            });
        }
    }
    true
}

async fn handle_mod_ban(
    target_wire_id: &str,
    hours: u32,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    if !is_caller_moderator(session, state).await {
        return session.send_error(ErrorCode::NotAllowed, "mod.ban");
    }
    let Some(target_uid) = parse_user_id(target_wire_id) else {
        return session.send_error(ErrorCode::Invalid, "mod.ban");
    };
    let until = if hours == 0 {
        None
    } else {
        Some(Utc::now() + chrono::Duration::hours(i64::from(hours)))
    };
    if state.users.set_ban(target_uid, until).await.is_err() {
        return session.send_error(ErrorCode::Invalid, "mod.ban");
    }
    if until.is_some() {
        state.hub.force_disconnect_user(target_wire_id);
    }
    true
}

async fn handle_mod_close_table(table_id: &str, session: &ConnSession, state: &AppState) -> bool {
    if !is_caller_moderator(session, state).await {
        return session.send_error(ErrorCode::NotAllowed, "mod.close_table");
    }
    let Some(table) = state.hub.find_table(table_id) else {
        return session.send_error(ErrorCode::Invalid, "mod.close_table");
    };
    table.try_send(TableCmd::ForceClose);
    true
}

async fn send_table_cmd(session: &ConnSession, state: &AppState, re: &str, cmd: TableCmd) -> bool {
    let Some(table) = active_table_handle(session, state) else {
        return session.send_error(ErrorCode::NotAllowed, re);
    };
    if !table.send(cmd).await {
        return session.send_error(ErrorCode::NotAllowed, re);
    }
    true
}

fn handle_chat_send(
    text: String,
    scope: Option<ChatScopeDto>,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    let active_table = active_table_handle(session, state);
    let target_scope = scope.unwrap_or_else(|| {
        if active_table.is_some() {
            ChatScopeDto::Table
        } else {
            ChatScopeDto::Lobby
        }
    });

    match target_scope {
        ChatScopeDto::Table => {
            let Some(table) = active_table else {
                return session.send_error(ErrorCode::NotAllowed, "chat.send");
            };
            table.try_send(TableCmd::SendChat {
                user_id: session.user.id.clone(),
                text,
            });
            true
        }
        ChatScopeDto::Lobby => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "chat.send");
            };
            room.try_send(LobbyCmd::SendChat {
                user_id: session.user.id.clone(),
                text,
            });
            true
        }
    }
}

fn handle_chat_ai_set(
    scope: ChatScopeDto,
    enabled: bool,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    match scope {
        ChatScopeDto::Table => {
            let Some(table) = active_table_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "chat.ai.set");
            };
            table.try_send(TableCmd::SetAiFriend {
                user_id: session.user.id.clone(),
                enabled,
            });
            true
        }
        ChatScopeDto::Lobby => {
            let Some(room) = active_room_handle(session, state) else {
                return session.send_error(ErrorCode::NotAllowed, "chat.ai.set");
            };
            room.try_send(LobbyCmd::SetAiFriend {
                user_id: session.user.id.clone(),
                enabled,
            });
            true
        }
    }
}

async fn handle_lobby_join(room_id: &str, session: &mut ConnSession, state: &AppState) -> bool {
    join_lobby_room(room_id, true, session, state).await
}

async fn join_lobby_room(
    room_id: &str,
    leave_active_table: bool,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    let Some(room) = state.hub.find_room(room_id) else {
        return session.send_error(ErrorCode::Invalid, "lobby.join");
    };

    // Leave any active table when switching to a different room.
    let switching_room = session.active_room.as_deref() != Some(room_id);
    if leave_active_table
        && switching_room
        && let Some(tid) = state.hub.user_table(&session.user.id)
        && let Some(table) = state.hub.find_table(&tid)
    {
        let _ = table
            .send(TableCmd::Leave {
                user_id: session.user.id.clone(),
            })
            .await;
    }
    if let Some(prev_id) = session.active_room.take()
        && prev_id != room_id
        && let Some(prev_room) = state.hub.find_room(&prev_id)
    {
        let _ = prev_room
            .send(LobbyCmd::Leave {
                user_id: session.user.id.clone(),
            })
            .await;
    }

    refresh_rating_for_game(room.game, session, state).await;

    let (tx, rx) = oneshot::channel();
    if !room
        .send(LobbyCmd::Join {
            user: session.user.clone(),
            sink: session.sink.clone(),
            ignored: session.ignored.clone(),
            muted: session.muted,
            reply: tx,
        })
        .await
    {
        return session.send_error(ErrorCode::Invalid, "lobby.join");
    }

    match rx.await {
        Ok(Ok(snapshot)) => {
            session.active_room = Some(room_id.to_owned());
            session.send_msg(&ServerMessage::LobbySnapshot(Box::new(snapshot)))
        }
        Ok(Err(code)) => session.send_error(code, "lobby.join"),
        Err(_) => session.send_error(ErrorCode::Invalid, "lobby.join"),
    }
}

async fn handle_table_join(
    table_id: &str,
    role: TableRoleDto,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    let Some(table) = state.hub.find_table(table_id) else {
        return session.send_error(ErrorCode::Invalid, "table.join");
    };

    // Ensure caller is in the table's owning room (enables direct `/#table=<id>` links).
    if session.active_room.as_deref() != Some(table.room_id.as_str())
        && !handle_lobby_join(&table.room_id, session, state).await
    {
        return false;
    }

    // Leave any different table first.
    if let Some(prev_tid) = state.hub.user_table(&session.user.id)
        && prev_tid != table_id
        && let Some(prev_table) = state.hub.find_table(&prev_tid)
    {
        let _ = prev_table
            .send(TableCmd::Leave {
                user_id: session.user.id.clone(),
            })
            .await;
    }

    let (tx, rx) = oneshot::channel();
    if !table
        .send(TableCmd::Join {
            client: session.to_table_client(),
            role,
            reply: tx,
        })
        .await
    {
        return session.send_error(ErrorCode::Invalid, "table.join");
    }

    match rx.await {
        Ok(Ok(snapshot)) => session.send_msg(&ServerMessage::TableSnapshot(Box::new(snapshot))),
        Ok(Err(code)) => session.send_error(code, "table.join"),
        Err(_) => session.send_error(ErrorCode::Invalid, "table.join"),
    }
}

async fn handle_ignore_set(
    target_wire_id: &str,
    on: bool,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    if target_wire_id == session.user.id {
        return session.send_error(ErrorCode::Invalid, "ignore.set");
    }
    if on {
        session.ignored.insert(target_wire_id.to_owned());
    } else {
        session.ignored.remove(target_wire_id);
    }

    if let ConnIdentity::Registered { user_id } = session.identity
        && let Some(target_uid) = parse_user_id(target_wire_id)
        && state
            .users
            .set_ignored(user_id, target_uid, on)
            .await
            .is_err()
    {
        return session.send_error(ErrorCode::Invalid, "ignore.set");
    }

    sync_member_update(session, state);
    true
}

async fn handle_auth_email(raw_email: &str, session: &ConnSession, state: &AppState) -> bool {
    if let Some(email) = validate_email(raw_email)
        && let Ok(token) = state.signer.sign_magic(&email)
    {
        let origin = state.config.public_origin.trim_end_matches('/');
        let login_url = format!("{origin}/#login={token}");
        if let Err(err) = state.mailer.send_magic_link(&email, &login_url).await {
            warn!(%err, "failed to dispatch magic-link email");
        }
    }
    session.send_msg(&ServerMessage::AuthSent {})
}

async fn handle_auth_magic(token: &str, session: &mut ConnSession, state: &AppState) -> bool {
    let Ok(claims) = state.signer.verify_magic(token) else {
        return session.send_error(ErrorCode::Invalid, "auth.magic");
    };
    complete_login(&claims.email, None, None, session, state, "auth.magic").await
}

async fn handle_auth_google(id_token: &str, session: &mut ConnSession, state: &AppState) -> bool {
    let Ok(verified) = state.google.verify_id_token(id_token).await else {
        return session.send_error(ErrorCode::Invalid, "auth.google");
    };
    complete_login(
        &verified.email,
        Some(&verified.sub),
        verified.suggested_name.as_deref(),
        session,
        state,
        "auth.google",
    )
    .await
}

async fn complete_login(
    email: &str,
    google_sub: Option<&str>,
    suggested_name: Option<&str>,
    session: &mut ConnSession,
    state: &AppState,
    re: &str,
) -> bool {
    let Ok((user, is_new)) = state
        .users
        .find_or_create_by_email(email, google_sub, suggested_name)
        .await
    else {
        return session.send_error(ErrorCode::Invalid, re);
    };
    if user.banned_until.is_some_and(|until| until > Utc::now()) {
        return session.send_error(ErrorCode::Banned, re);
    }
    let Ok(token) = state.signer.sign_session(user.id, user.session_version) else {
        return session.send_error(ErrorCode::Invalid, re);
    };

    let me = user_record_to_dto(&user, 1200, 0, UserStatusDto::Available);
    switch_identity(
        session,
        ConnIdentity::Registered { user_id: user.id },
        me.clone(),
        is_muted(&user),
        ignored_wire_set(&user),
        state,
    )
    .await;

    session.send_msg(&ServerMessage::AuthOk {
        session: token,
        me,
        is_new,
    })
}

async fn handle_auth_logout(session: &mut ConnSession, state: &AppState) -> bool {
    let guest_num = state.allocate_guest_num();
    let guest_tok = state.signer.sign_guest(guest_num).ok();
    let me = guest_to_user_dto(guest_num, UserStatusDto::Available);
    switch_identity(
        session,
        ConnIdentity::Guest,
        me.clone(),
        false,
        HashSet::new(),
        state,
    )
    .await;

    session.send_msg(&ServerMessage::Welcome {
        protocol_version: PROTOCOL_VERSION,
        mode: SessionKindDto::Remote,
        me: Some(me),
        guest: guest_tok,
        rooms: state.default_rooms(),
    })
}

async fn switch_identity(
    session: &mut ConnSession,
    new_identity: ConnIdentity,
    new_user: UserDto,
    new_muted: bool,
    new_ignored: HashSet<String>,
    state: &AppState,
) {
    let previous_room = session.active_room.clone();
    cleanup_session(session, state, false);

    session.identity = new_identity;
    session.user = new_user.clone();
    session.muted = new_muted;
    session.ignored = new_ignored;
    session.active_room = None;
    session.sink = ClientSink::new(
        new_user.id,
        session.out_tx.clone(),
        Arc::clone(&session.slow_disconnect),
        Arc::clone(&session.disconnect_notify),
    );
    state.hub.register_connection(session.sink.clone());

    if let Some(room_id) = previous_room {
        let _ = handle_lobby_join(&room_id, session, state).await;
    }
}

async fn handle_account_update(
    name: Option<&str>,
    avatar: Option<u8>,
    session: &mut ConnSession,
    state: &AppState,
) -> bool {
    let ConnIdentity::Registered { user_id } = session.identity else {
        return session.send_error(ErrorCode::NotAllowed, "account.update");
    };
    let avatar_i16 = avatar.map(i16::from);
    match state
        .users
        .update_account(user_id, name, avatar_i16, Utc::now())
        .await
    {
        Ok(user) => {
            let Ok(token) = state.signer.sign_session(user.id, user.session_version) else {
                return session.send_error(ErrorCode::Invalid, "account.update");
            };
            let me = user_record_to_dto(&user, session.user.rating, 0, session.user.status);
            session.user.name.clone_from(&me.name);
            session.user.avatar = me.avatar;
            sync_member_update(session, state);
            session.send_msg(&ServerMessage::AuthOk {
                session: token,
                me: session.user.clone(),
                is_new: false,
            })
        }
        Err(PortError::Conflict(_) | PortError::Invalid(_)) => {
            session.send_error(ErrorCode::Invalid, "account.update")
        }
        Err(_) => session.send_error(ErrorCode::NotAllowed, "account.update"),
    }
}

async fn handle_account_delete(session: &ConnSession, state: &AppState) -> bool {
    let ConnIdentity::Registered { user_id } = session.identity else {
        return session.send_error(ErrorCode::NotAllowed, "account.delete");
    };
    if state.users.delete_user(user_id).await.is_err() {
        return session.send_error(ErrorCode::Invalid, "account.delete");
    }
    false
}

async fn handle_profile_get(wire_id: &str, session: &ConnSession, state: &AppState) -> bool {
    let Some(uid) = parse_user_id(wire_id) else {
        return session.send_error(ErrorCode::Invalid, "profile.get");
    };
    let Ok(Some(user)) = state.users.find_by_id(uid).await else {
        return session.send_error(ErrorCode::Invalid, "profile.get");
    };
    let user_stats = state.stats.stats_for_user(uid).await.unwrap_or_default();
    let recent = state
        .matches
        .recent_matches(uid, 20)
        .await
        .unwrap_or_default();

    let (headline_rating, headline_rated_games) = user_stats
        .first()
        .map_or((1200, 0), |s| (s.rating, s.rated_games));

    session.send_msg(&ServerMessage::Profile {
        user: user_record_to_dto(
            &user,
            headline_rating,
            headline_rated_games,
            UserStatusDto::Available,
        ),
        since: user.created_at.timestamp_millis(),
        stats: user_stats.iter().map(stat_record_to_profile_dto).collect(),
        last: recent.iter().map(match_item_to_dto).collect(),
    })
}

async fn handle_highscores_get(
    game: GameKindDto,
    around: bool,
    session: &ConnSession,
    state: &AppState,
) -> bool {
    let around_uid = if around {
        match session.identity {
            ConnIdentity::Registered { user_id } => Some(user_id),
            ConnIdentity::Guest => None,
        }
    } else {
        None
    };
    match state
        .stats
        .highscores(game_kind_from_dto(game), around_uid)
        .await
    {
        Ok(entries) => session.send_msg(&ServerMessage::Highscores {
            game,
            rows: entries.iter().map(highscore_entry_to_dto).collect(),
        }),
        Err(_) => session.send_error(ErrorCode::Invalid, "highscores.get"),
    }
}

async fn refresh_rating_for_game(game: GameKindDto, session: &mut ConnSession, state: &AppState) {
    let ConnIdentity::Registered { user_id } = session.identity else {
        return;
    };
    let domain_game = game_kind_from_dto(game);
    if let Ok(stats) = state.stats.stats_for_user(user_id).await {
        if let Some(row) = stats.into_iter().find(|s| s.game == domain_game) {
            session.user.rating = row.rating;
            session.user.prov = row.rated_games < 10;
        } else {
            session.user.rating = 1200;
            session.user.prov = true;
        }
    }
}

fn sync_member_update(session: &ConnSession, state: &AppState) {
    if let Some(room) = active_room_handle(session, state) {
        room.try_send(LobbyCmd::UpdateUser {
            user: session.user.clone(),
            ignored: session.ignored.clone(),
            muted: session.muted,
        });
    }
    if let Some(table) = active_table_handle(session, state) {
        table.try_send(TableCmd::UpdateMemberUser {
            user: session.user.clone(),
            muted: session.muted,
        });
    }
}

fn active_room_handle(
    session: &ConnSession,
    state: &AppState,
) -> Option<crate::actors::lobby_actor::LobbyHandle> {
    session
        .active_room
        .as_deref()
        .and_then(|rid| state.hub.find_room(rid))
}

fn active_table_handle(
    session: &ConnSession,
    state: &AppState,
) -> Option<crate::actors::table_actor::TableHandle> {
    state
        .hub
        .user_table(&session.user.id)
        .and_then(|tid| state.hub.find_table(&tid))
}

fn is_muted(user: &UserRecord) -> bool {
    user.muted_until.is_some_and(|until| until > Utc::now())
}

fn ignored_wire_set(user: &UserRecord) -> HashSet<String> {
    user.ignored_ids.iter().map(|id| format!("u{id}")).collect()
}

fn send_raw_error(out_tx: &mpsc::Sender<Message>, code: ErrorCode, re: &str) -> bool {
    let msg = ServerMessage::Error {
        code,
        re: re.to_owned(),
    };
    let Ok(json) = serde_json::to_string(&msg) else {
        return false;
    };
    out_tx.try_send(Message::Text(json.into())).is_ok()
}

struct OnlineGuard {
    state: AppState,
}

impl Drop for OnlineGuard {
    fn drop(&mut self) {
        self.state.online.fetch_sub(1, Ordering::Relaxed);
    }
}

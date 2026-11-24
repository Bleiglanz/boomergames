//! WebSocket connection handler enforcing handshake, 4 KB frame cap, rate limits,
//! 20s ping/pong liveness, non-blocking outbound queues, and passwordless auth.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use boomer_protocol::{
    ClientMessage, ErrorCode, PROTOCOL_VERSION, ServerMessage, SessionKindDto, UserStatusDto,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::auth::validate_email;
use crate::mapping::{
    game_kind_from_dto, guest_to_user_dto, highscore_entry_to_dto, match_item_to_dto,
    parse_user_id, stat_record_to_profile_dto, user_record_to_dto,
};
use crate::ports::PortError;
use crate::state::AppState;

/// Maximum inbound JSON text frame size (4 KB per `incubation/plan.md` §9.6).
const MAX_MESSAGE_BYTES: usize = 4_096;

/// Bounded per-client outbound buffer; slow clients overflowing it are disconnected cleanly.
const OUTBOUND_QUEUE_CAP: usize = 64;

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

    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if ws_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    // Step 1: First message MUST be `hello` within 10 seconds (§9.6).
    let Some(mut identity) = perform_handshake(&mut ws_rx, &out_tx, &state).await else {
        drop(out_tx);
        let _ = writer.await;
        return;
    };

    // Step 2: Authenticated / Guest message loop with 20s ping/pong liveness.
    let mut ping_ticker = tokio::time::interval(PING_INTERVAL);
    ping_ticker.tick().await; // consume immediate tick
    let mut missed_pongs: u8 = 0;
    let mut limiter = RateLimiter::new();

    loop {
        tokio::select! {
            _ = ping_ticker.tick() => {
                if missed_pongs >= MAX_MISSED_PONGS {
                    debug!("closing silent WebSocket after missed pongs");
                    break;
                }
                missed_pongs = missed_pongs.saturating_add(1);
                if out_tx.try_send(Message::Ping(Vec::new().into())).is_err() {
                    debug!("disconnecting slow WebSocket client on ping backpressure");
                    break;
                }
            }
            maybe_frame = ws_rx.next() => {
                let Some(Ok(frame)) = maybe_frame else {
                    break;
                };
                match frame {
                    Message::Pong(_) => {
                        missed_pongs = 0;
                    }
                    Message::Ping(payload) => {
                        if out_tx.try_send(Message::Pong(payload)).is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) => {
                        if !send_error(&out_tx, ErrorCode::Invalid, "binary") {
                            break;
                        }
                    }
                    Message::Text(text) => {
                        if text.len() > MAX_MESSAGE_BYTES {
                            if !send_error(&out_tx, ErrorCode::Invalid, "frame") {
                                break;
                            }
                            continue;
                        }
                        if let Err(should_disconnect) = limiter.allow() {
                            let _ = send_error(&out_tx, ErrorCode::RateLimited, "rate");
                            if should_disconnect {
                                break;
                            }
                            continue;
                        }
                        let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) else {
                            if !send_error(&out_tx, ErrorCode::Invalid, "parse") {
                                break;
                            }
                            continue;
                        };
                        let keep_open = handle_client_message(
                            client_msg,
                            &mut identity,
                            &out_tx,
                            &state,
                        )
                        .await;
                        if !keep_open {
                            break;
                        }
                    }
                }
            }
        }
    }

    let _ = out_tx.try_send(Message::Close(None));
    drop(out_tx);
    let _ = writer.await;
}

async fn perform_handshake(
    ws_rx: &mut futures_util::stream::SplitStream<WebSocket>,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> Option<ConnIdentity> {
    let first = tokio::time::timeout(Duration::from_secs(10), ws_rx.next())
        .await
        .ok()??
        .ok()?;
    let Message::Text(text) = first else {
        return None;
    };
    if text.len() > MAX_MESSAGE_BYTES {
        let _ = send_error(out_tx, ErrorCode::Invalid, "hello");
        return None;
    }
    let Ok(ClientMessage::Hello { v, session, guest }) =
        serde_json::from_str::<ClientMessage>(&text)
    else {
        let _ = send_error(out_tx, ErrorCode::NotAllowed, "hello");
        return None;
    };

    if v != PROTOCOL_VERSION {
        let _ = send_error(out_tx, ErrorCode::UpgradeRequired, "hello");
        return None;
    }

    // Try resuming a signed registered session token.
    if let Some(ref token) = session
        && let Ok(claims) = state.signer.verify_session(token)
        && let Ok(Some(user)) = state.users.find_by_id(claims.user_id).await
    {
        if user.banned_until.is_some_and(|until| until > Utc::now()) {
            let _ = send_error(out_tx, ErrorCode::Banned, "hello");
            return None;
        }
        if user.session_version == claims.session_version {
            let me = user_record_to_dto(&user, 1200, 0, UserStatusDto::Available);
            let _ = send_msg(
                out_tx,
                &ServerMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    mode: SessionKindDto::Remote,
                    me: Some(me),
                    guest: None,
                    rooms: state.default_rooms(),
                },
            );
            return Some(ConnIdentity::Registered { user_id: user.id });
        }
    }

    // Otherwise resume a valid guest reconnect token or issue a new guest identity.
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
    if !send_msg(
        out_tx,
        &ServerMessage::Welcome {
            protocol_version: PROTOCOL_VERSION,
            mode: SessionKindDto::Remote,
            me: Some(me),
            guest: issued_guest_token,
            rooms: state.default_rooms(),
        },
    ) {
        return None;
    }
    Some(ConnIdentity::Guest)
}

async fn handle_client_message(
    msg: ClientMessage,
    identity: &mut ConnIdentity,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    match msg {
        ClientMessage::AuthEmail { email } => handle_auth_email(&email, out_tx, state).await,
        ClientMessage::AuthMagic { token } => {
            handle_auth_magic(&token, identity, out_tx, state).await
        }
        ClientMessage::AuthGoogle { id_token } => {
            handle_auth_google(&id_token, identity, out_tx, state).await
        }
        ClientMessage::AuthLogout {} => {
            let guest_num = state.allocate_guest_num();
            let guest_tok = state.signer.sign_guest(guest_num).ok();
            *identity = ConnIdentity::Guest;
            send_msg(
                out_tx,
                &ServerMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    mode: SessionKindDto::Remote,
                    me: Some(guest_to_user_dto(guest_num, UserStatusDto::Available)),
                    guest: guest_tok,
                    rooms: state.default_rooms(),
                },
            )
        }
        ClientMessage::AccountUpdate { name, avatar } => {
            handle_account_update(name.as_deref(), avatar, identity, out_tx, state).await
        }
        ClientMessage::AccountDelete {} => {
            let ConnIdentity::Registered { user_id } = *identity else {
                return send_error(out_tx, ErrorCode::NotAllowed, "account.delete");
            };
            if state.users.delete_user(user_id).await.is_err() {
                return send_error(out_tx, ErrorCode::Invalid, "account.delete");
            }
            // Spec §9.6: server closes the connection after deleting the account.
            false
        }
        ClientMessage::ProfileGet { user_id } => handle_profile_get(&user_id, out_tx, state).await,
        ClientMessage::HighscoresGet { game, around } => {
            let around_uid = if around {
                match *identity {
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
                Ok(entries) => send_msg(
                    out_tx,
                    &ServerMessage::Highscores {
                        game,
                        rows: entries.iter().map(highscore_entry_to_dto).collect(),
                    },
                ),
                Err(_) => send_error(out_tx, ErrorCode::Invalid, "highscores.get"),
            }
        }
        ClientMessage::Hello { .. } => send_error(out_tx, ErrorCode::NotAllowed, "hello"),
        _ => send_error(out_tx, ErrorCode::NotAllowed, "unsupported"),
    }
}

async fn handle_auth_email(
    raw_email: &str,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    if let Some(email) = validate_email(raw_email)
        && let Ok(token) = state.signer.sign_magic(&email)
    {
        let origin = state.config.public_origin.trim_end_matches('/');
        let login_url = format!("{origin}/#login={token}");
        if let Err(err) = state.mailer.send_magic_link(&email, &login_url).await {
            warn!(%err, "failed to dispatch magic-link email");
        }
    }
    // Always reply `auth.sent` to avoid leaking whether an email is registered (§9.6).
    send_msg(out_tx, &ServerMessage::AuthSent {})
}

async fn handle_auth_magic(
    token: &str,
    identity: &mut ConnIdentity,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    let Ok(claims) = state.signer.verify_magic(token) else {
        return send_error(out_tx, ErrorCode::Invalid, "auth.magic");
    };
    complete_login(
        &claims.email,
        None,
        None,
        identity,
        out_tx,
        state,
        "auth.magic",
    )
    .await
}

async fn handle_auth_google(
    id_token: &str,
    identity: &mut ConnIdentity,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    let Ok(verified) = state.google.verify_id_token(id_token).await else {
        return send_error(out_tx, ErrorCode::Invalid, "auth.google");
    };
    complete_login(
        &verified.email,
        Some(&verified.sub),
        verified.suggested_name.as_deref(),
        identity,
        out_tx,
        state,
        "auth.google",
    )
    .await
}

async fn complete_login(
    email: &str,
    google_sub: Option<&str>,
    suggested_name: Option<&str>,
    identity: &mut ConnIdentity,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
    re: &str,
) -> bool {
    let Ok((user, is_new)) = state
        .users
        .find_or_create_by_email(email, google_sub, suggested_name)
        .await
    else {
        return send_error(out_tx, ErrorCode::Invalid, re);
    };
    if user.banned_until.is_some_and(|until| until > Utc::now()) {
        return send_error(out_tx, ErrorCode::Banned, re);
    }
    let Ok(session) = state.signer.sign_session(user.id, user.session_version) else {
        return send_error(out_tx, ErrorCode::Invalid, re);
    };
    *identity = ConnIdentity::Registered { user_id: user.id };
    let me = user_record_to_dto(&user, 1200, 0, UserStatusDto::Available);
    send_msg(
        out_tx,
        &ServerMessage::AuthOk {
            session,
            me,
            is_new,
        },
    )
}

async fn handle_account_update(
    name: Option<&str>,
    avatar: Option<u8>,
    identity: &ConnIdentity,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    let ConnIdentity::Registered { user_id } = *identity else {
        return send_error(out_tx, ErrorCode::NotAllowed, "account.update");
    };
    let avatar_i16 = avatar.map(i16::from);
    match state
        .users
        .update_account(user_id, name, avatar_i16, Utc::now())
        .await
    {
        Ok(user) => {
            let Ok(session) = state.signer.sign_session(user.id, user.session_version) else {
                return send_error(out_tx, ErrorCode::Invalid, "account.update");
            };
            let me = user_record_to_dto(&user, 1200, 0, UserStatusDto::Available);
            send_msg(
                out_tx,
                &ServerMessage::AuthOk {
                    session,
                    me,
                    is_new: false,
                },
            )
        }
        Err(PortError::Conflict(_) | PortError::Invalid(_)) => {
            send_error(out_tx, ErrorCode::Invalid, "account.update")
        }
        Err(_) => send_error(out_tx, ErrorCode::NotAllowed, "account.update"),
    }
}

async fn handle_profile_get(
    wire_id: &str,
    out_tx: &mpsc::Sender<Message>,
    state: &AppState,
) -> bool {
    let Some(uid) = parse_user_id(wire_id) else {
        return send_error(out_tx, ErrorCode::Invalid, "profile.get");
    };
    let Ok(Some(user)) = state.users.find_by_id(uid).await else {
        return send_error(out_tx, ErrorCode::Invalid, "profile.get");
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

    send_msg(
        out_tx,
        &ServerMessage::Profile {
            user: user_record_to_dto(
                &user,
                headline_rating,
                headline_rated_games,
                UserStatusDto::Available,
            ),
            since: user.created_at.timestamp_millis(),
            stats: user_stats.iter().map(stat_record_to_profile_dto).collect(),
            last: recent.iter().map(match_item_to_dto).collect(),
        },
    )
}

fn send_msg(out_tx: &mpsc::Sender<Message>, msg: &ServerMessage) -> bool {
    let Ok(json) = serde_json::to_string(msg) else {
        return false;
    };
    // Non-blocking `try_send`: slow clients that overflow the bounded queue disconnect cleanly.
    out_tx.try_send(Message::Text(json.into())).is_ok()
}

fn send_error(out_tx: &mpsc::Sender<Message>, code: ErrorCode, re: &str) -> bool {
    send_msg(
        out_tx,
        &ServerMessage::Error {
            code,
            re: re.to_owned(),
        },
    )
}

struct OnlineGuard {
    state: AppState,
}

impl Drop for OnlineGuard {
    fn drop(&mut self) {
        self.state.online.fetch_sub(1, Ordering::Relaxed);
    }
}

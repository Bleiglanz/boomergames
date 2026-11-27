//! Single-room lounge actor (`tokio` task per room, zero shared locks).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use boomer_protocol::{
    BotLevelDto, ChatLineDto, ChatScopeDto, ErrorCode, GameKindDto, InviteCloseReasonDto,
    LobbySnapshotDto, ServerMessage, TablePhaseDto, TableSettingsDto, TableSummaryDto,
    TableVisibilityDto, UserDto, UserStatusDto,
};
use chrono::Utc;
use tokio::sync::{mpsc, oneshot};

use crate::actors::ai_chat::{
    AiChatState, game_context_name, make_ai_chat_line, recent_chat_turns, sanitize_ai_reply,
};
use crate::actors::hub::LobbyHub;
use crate::actors::table_actor::{TableActor, TableClient, TableCmd, TableInit};
use crate::actors::{ClientSink, MAX_CHAT_HISTORY, broadcast_msg, sanitize_chat_text};
use crate::ports::{AiChatContext, PortError};

/// Room chat rate limit (1 msg/s per `incubation/plan.md` §9.6).
const CHAT_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Direct invite lifetime (30 seconds per Milestone 5.2).
const INVITE_TTL: Duration = Duration::from_secs(30);

/// Initial Elo matching window for `Play Now` (`±200` Elo).
const PLAY_NOW_BASE_WINDOW: i32 = 200;

/// Additional Elo window expansion every 5 seconds.
const PLAY_NOW_WIDEN_STEP: i32 = 100;

/// Human-first wait duration in `Play Now` before suggesting a bot (20 seconds).
const PLAY_NOW_WAIT_TIMEOUT: Duration = Duration::from_secs(20);

/// Bounded command mailbox capacity for each [`LobbyActor`].
const LOBBY_CMD_CAPACITY: usize = 256;

/// Bounded event channel capacity for fire-and-forget updates from [`TableActor`]s.
const LOBBY_EVENT_CAPACITY: usize = 256;

/// Cloneable handle for sending downward commands to a [`LobbyActor`].
#[derive(Debug, Clone)]
pub struct LobbyHandle {
    /// Room ID (e.g., `"pool-social-1"`).
    pub room_id: String,
    /// Game hosted in this room.
    pub game: GameKindDto,
    cmd_tx: mpsc::Sender<LobbyCmd>,
}

impl LobbyHandle {
    /// Sends a downward command to this room actor.
    pub async fn send(&self, cmd: LobbyCmd) -> bool {
        self.cmd_tx.send(cmd).await.is_ok()
    }

    /// Sends a command without awaiting (used for cleanup on disconnect).
    pub fn try_send(&self, cmd: LobbyCmd) {
        let _ = self.cmd_tx.try_send(cmd);
    }
}

/// Fire-and-forget upward notification sent from a [`TableActor`] to its owning [`LobbyActor`].
pub enum LobbyEvent {
    /// Table seats, settings, watchers, or phase changed.
    TableSummaryUpdated(TableSummaryDto),
    /// Table became empty and shut down.
    TableClosed {
        /// Closed table ID.
        table_id: String,
    },
    /// Member presence status changed because they joined/left/started at a table.
    UserPresenceChanged {
        /// Affected user wire ID.
        user_id: String,
        /// Updated presence status.
        status: UserStatusDto,
        /// Active table ID, if any.
        table_id: Option<String>,
    },
    /// Member Elo rating updated after a rated match finished.
    UserRatingUpdated {
        /// Affected user wire ID.
        user_id: String,
        /// New per-game Elo rating.
        rating: i32,
        /// Updated provisional flag.
        prov: bool,
    },
}

/// Downward commands processed by a [`LobbyActor`].
pub enum LobbyCmd {
    /// Join this room and receive a complete [`LobbySnapshotDto`].
    Join {
        /// Joining user metadata.
        user: UserDto,
        /// Non-blocking outbound sink.
        sink: ClientSink,
        /// Ignored wire IDs for this user.
        ignored: HashSet<String>,
        /// Moderation mute status.
        muted: bool,
        /// Reply channel delivering the initial snapshot.
        reply: oneshot::Sender<Result<LobbySnapshotDto, ErrorCode>>,
    },
    /// Leave this room (switching rooms or disconnecting).
    Leave {
        /// Departing user wire ID.
        user_id: String,
    },
    /// Refresh user metadata (after sign-in, rename, avatar update, or ignore change).
    UpdateUser {
        /// Updated user info.
        user: UserDto,
        /// Updated ignore list.
        ignored: HashSet<String>,
        /// Updated mute state.
        muted: bool,
    },
    /// Send a room chat line.
    SendChat {
        /// Sender wire ID.
        user_id: String,
        /// Raw message text.
        text: String,
    },
    /// Toggle the optional Claude AI Friend on or off for this lounge room.
    SetAiFriend {
        /// Requesting member wire ID.
        user_id: String,
        /// Desired enabled state.
        enabled: bool,
    },
    /// Internal callback delivering an asynchronously generated AI Friend chat reply.
    AiReply {
        /// Request generation counter used to discard stale replies after toggle/reset.
        generation: u64,
        /// Generated reply text or error from [`crate::ports::AiFriend`].
        result: Result<String, PortError>,
    },
    /// Create a new table in this room and seat the caller as host.
    CreateTable {
        /// Creator wire ID.
        user_id: String,
        /// Requested table settings.
        settings: TableSettingsDto,
    },
    /// Quick-play matchmaking (human-first or instant `vsBot`).
    PlayNow {
        /// Caller wire ID.
        user_id: String,
        /// Optional computer opponent level.
        vs_bot: Option<BotLevelDto>,
    },
    /// Send a 30-second direct invite to another available user in this room.
    SendInvite {
        /// Inviting user wire ID.
        from_id: String,
        /// Target user wire ID.
        to_id: String,
        /// Optional existing table ID.
        table_id: Option<String>,
        /// Optional settings for a new table.
        settings: Option<TableSettingsDto>,
    },
    /// Accept or decline a pending invite.
    AnswerInvite {
        /// Answering user wire ID.
        user_id: String,
        /// Pending invite identifier.
        invite_id: String,
        /// `true` to accept, `false` to decline.
        accept: bool,
    },
    /// Exports recent room chat lines for a moderation report snapshot.
    DumpChat {
        /// Reply channel returning formatted chat transcript.
        reply: oneshot::Sender<String>,
    },
}

struct LobbyMember {
    user: UserDto,
    sink: ClientSink,
    ignored: HashSet<String>,
    muted: bool,
    active_table: Option<String>,
    last_chat_at: Option<Instant>,
}

struct PendingInvite {
    id: String,
    from_id: String,
    to_id: String,
    table_id: Option<String>,
    settings: TableSettingsDto,
    created_at: Instant,
}

struct PlayNowTicket {
    user_id: String,
    rating: i32,
    guest: bool,
    queued_at: Instant,
}

/// Authoritative actor owning one lounge room's roster, tables summary, chat, invites, and `Play Now` queue.
pub struct LobbyActor {
    room_id: String,
    game: GameKindDto,
    capacity: usize,
    members: HashMap<String, LobbyMember>,
    tables: HashMap<String, TableSummaryDto>,
    chat: VecDeque<ChatLineDto>,
    ai_chat: AiChatState,
    invites: HashMap<String, PendingInvite>,
    play_now_queue: Vec<PlayNowTicket>,
    self_tx: mpsc::Sender<LobbyCmd>,
    cmd_rx: mpsc::Receiver<LobbyCmd>,
    event_tx: mpsc::Sender<LobbyEvent>,
    event_rx: mpsc::Receiver<LobbyEvent>,
    hub: Arc<LobbyHub>,
}

impl LobbyActor {
    /// Spawns a new [`LobbyActor`] task for `room_id` and returns its [`LobbyHandle`].
    #[must_use]
    pub fn spawn(
        room_id: String,
        game: GameKindDto,
        capacity: usize,
        hub: Arc<LobbyHub>,
    ) -> LobbyHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(LOBBY_CMD_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(LOBBY_EVENT_CAPACITY);
        let self_tx = cmd_tx.clone();
        let handle = LobbyHandle {
            room_id: room_id.clone(),
            game,
            cmd_tx,
        };
        // Social Lounges default to AI Friend ON whenever an Anthropic key is configured.
        let ai_default_on = hub.ai_friend().is_configured();
        let mut actor = Self {
            room_id,
            game,
            capacity,
            members: HashMap::new(),
            tables: HashMap::new(),
            chat: VecDeque::with_capacity(MAX_CHAT_HISTORY),
            ai_chat: AiChatState::new(ai_default_on),
            invites: HashMap::new(),
            play_now_queue: Vec::new(),
            self_tx,
            cmd_rx,
            event_tx,
            event_rx,
            hub,
        };
        actor.spawn_house_bot_tables();
        tokio::spawn(actor.run());
        handle
    }

    fn spawn_house_bot_tables(&mut self) {
        for level in [BotLevelDto::Easy, BotLevelDto::Medium, BotLevelDto::Hard] {
            let no = self.hub.alloc_table_no();
            let id = format!("t{no}");
            let settings = TableSettingsDto {
                visibility: TableVisibilityDto::Public,
                rated: false,
                timer: 30,
                mute_spectators: false,
                opts: boomer_protocol::TableGameOptsDto::default(),
            };
            let (_handle, summary, _snap) = TableActor::spawn(
                TableInit {
                    id: id.clone(),
                    no,
                    room_id: self.room_id.clone(),
                    game: self.game,
                    settings,
                    host: None,
                    second_human: None,
                    second_bot: Some(level),
                    is_house_bot: true,
                },
                self.event_tx.clone(),
                &self.hub,
            );
            self.tables.insert(id, summary);
        }
    }

    async fn run(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                Some(cmd) = self.cmd_rx.recv() => {
                    self.handle_cmd(cmd);
                }
                Some(ev) = self.event_rx.recv() => {
                    self.handle_event(ev);
                }
                _ = ticker.tick() => {
                    self.on_tick();
                }
                else => break,
            }
        }
    }

    fn handle_cmd(&mut self, cmd: LobbyCmd) {
        match cmd {
            LobbyCmd::Join {
                user,
                sink,
                ignored,
                muted,
                reply,
            } => {
                let res = self.handle_join(user, sink, ignored, muted);
                let _ = reply.send(res);
            }
            LobbyCmd::Leave { user_id } => self.handle_leave(&user_id),
            LobbyCmd::UpdateUser {
                user,
                ignored,
                muted,
            } => self.handle_update_user(user, ignored, muted),
            LobbyCmd::SendChat { user_id, text } => self.handle_chat(&user_id, &text),
            LobbyCmd::SetAiFriend { user_id, enabled } => {
                self.handle_set_ai_friend(&user_id, enabled);
            }
            LobbyCmd::AiReply { generation, result } => {
                self.handle_ai_reply(generation, result);
            }
            LobbyCmd::CreateTable { user_id, settings } => {
                self.handle_create_table(&user_id, settings);
            }
            LobbyCmd::PlayNow { user_id, vs_bot } => self.handle_play_now(&user_id, vs_bot),
            LobbyCmd::SendInvite {
                from_id,
                to_id,
                table_id,
                settings,
            } => self.handle_send_invite(&from_id, &to_id, table_id, settings),
            LobbyCmd::AnswerInvite {
                user_id,
                invite_id,
                accept,
            } => self.handle_answer_invite(&user_id, &invite_id, accept),
            LobbyCmd::DumpChat { reply } => {
                let transcript = self
                    .chat
                    .iter()
                    .map(|l| format!("{}: {}", l.name.as_deref().unwrap_or("system"), l.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = reply.send(transcript);
            }
        }
    }

    fn handle_event(&mut self, ev: LobbyEvent) {
        match ev {
            LobbyEvent::TableSummaryUpdated(summary) => {
                self.tables.insert(summary.id.clone(), summary.clone());
                let msg = ServerMessage::TableUpsert(Box::new(summary));
                self.broadcast_all(&msg);
            }
            LobbyEvent::TableClosed { table_id } => {
                if self.tables.remove(&table_id).is_some() {
                    self.broadcast_all(&ServerMessage::TableRemove { id: table_id });
                }
            }
            LobbyEvent::UserPresenceChanged {
                user_id,
                status,
                table_id,
            } => {
                if let Some(member) = self.members.get_mut(&user_id) {
                    member.user.status = status;
                    member.active_table = table_id;
                    let dto = member.user.clone();
                    self.broadcast_all(&ServerMessage::UserUpdate(dto));
                }
            }
            LobbyEvent::UserRatingUpdated {
                user_id,
                rating,
                prov,
            } => {
                if let Some(member) = self.members.get_mut(&user_id) {
                    member.user.rating = rating;
                    member.user.prov = prov;
                    let dto = member.user.clone();
                    self.broadcast_all(&ServerMessage::UserUpdate(dto));
                }
            }
        }
    }

    fn handle_join(
        &mut self,
        mut user: UserDto,
        sink: ClientSink,
        ignored: HashSet<String>,
        muted: bool,
    ) -> Result<LobbySnapshotDto, ErrorCode> {
        let already_in = self.members.contains_key(&user.id);
        if !already_in && self.members.len() >= self.capacity {
            return Err(ErrorCode::TableFull);
        }

        user.status = UserStatusDto::Available;
        let uid = user.id.clone();
        let ignored_vec: Vec<String> = ignored.iter().cloned().collect();

        if !already_in {
            self.broadcast_all(&ServerMessage::UserAdd(user.clone()));
        }

        self.members.insert(
            uid,
            LobbyMember {
                user: user.clone(),
                sink,
                ignored,
                muted,
                active_table: None,
                last_chat_at: None,
            },
        );

        let count = u32::try_from(self.members.len()).unwrap_or(u32::MAX);
        self.hub.update_room_users(&self.room_id, count);
        self.hub.ensure_room_capacity(self.game);

        Ok(LobbySnapshotDto {
            room: self.room_id.clone(),
            game: self.game,
            me: user,
            users: self.sorted_users(),
            tables: self.sorted_tables(),
            chat: self.chat.iter().cloned().collect(),
            online: self.hub.online_count(),
            ignored: ignored_vec,
            ai_friend_available: self.hub.ai_friend().is_configured(),
            ai_friend: self.ai_chat.is_enabled(),
        })
    }

    fn handle_leave(&mut self, user_id: &str) {
        if self.members.remove(user_id).is_none() {
            return;
        }
        self.play_now_queue.retain(|t| t.user_id != user_id);
        self.cancel_invites_for(user_id);

        let count = u32::try_from(self.members.len()).unwrap_or(u32::MAX);
        self.hub.update_room_users(&self.room_id, count);
        self.broadcast_all(&ServerMessage::UserRemove {
            id: user_id.to_owned(),
        });
    }

    fn handle_update_user(&mut self, mut user: UserDto, ignored: HashSet<String>, muted: bool) {
        if let Some(member) = self.members.get_mut(&user.id) {
            user.status = member.user.status;
            member.user = user.clone();
            member.ignored = ignored;
            member.muted = muted;
            self.broadcast_all(&ServerMessage::UserUpdate(user));
        }
    }

    fn handle_chat(&mut self, user_id: &str, raw_text: &str) {
        let Some(member) = self.members.get_mut(user_id) else {
            return;
        };
        let sink = member.sink.clone();
        if member.muted {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::Muted,
                re: "chat.send".to_owned(),
            });
            return;
        }
        let now = Instant::now();
        if let Some(last) = member.last_chat_at
            && now.duration_since(last) < CHAT_MIN_INTERVAL
        {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::RateLimited,
                re: "chat.send".to_owned(),
            });
            return;
        }
        let Some(clean) = sanitize_chat_text(raw_text) else {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::Invalid,
                re: "chat.send".to_owned(),
            });
            return;
        };
        member.last_chat_at = Some(now);
        let line = ChatLineDto {
            from: Some(user_id.to_owned()),
            name: Some(member.user.name.clone()),
            text: clean,
            ts: Utc::now().timestamp_millis(),
            sys: false,
            ai: false,
        };
        self.push_chat_and_broadcast(line);
        self.maybe_trigger_ai_reply();
    }

    fn handle_set_ai_friend(&mut self, user_id: &str, enabled: bool) {
        let Some(member) = self.members.get(user_id) else {
            return;
        };
        let sink = member.sink.clone();
        let actor_name = member.user.name.clone();
        if member.muted {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::Muted,
                re: "chat.ai.set".to_owned(),
            });
            return;
        }
        if !self.hub.ai_friend().is_configured() {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::NotAllowed,
                re: "chat.ai.set".to_owned(),
            });
            return;
        }
        if self.ai_chat.set_enabled(enabled) {
            self.broadcast_all(&ServerMessage::ChatAiState {
                scope: ChatScopeDto::Lobby,
                enabled,
            });
            let state_word = if enabled { "on" } else { "off" };
            self.push_system_chat(&format!("{actor_name} turned AI Friend {state_word}."));
        } else {
            let _ = sink.send_msg(&ServerMessage::ChatAiState {
                scope: ChatScopeDto::Lobby,
                enabled,
            });
        }
    }

    fn maybe_trigger_ai_reply(&mut self) {
        let ai_friend = self.hub.ai_friend();
        if !ai_friend.is_configured() {
            return;
        }
        let cooldown = self.hub.ai_reply_cooldown();
        let solo_human = self.members.len() <= 1;
        let Some(generation) = self
            .ai_chat
            .on_human_message(Instant::now(), cooldown, solo_human)
        else {
            return;
        };
        let ctx = AiChatContext {
            setting: format!(
                "{} Social Lounge ({}, {} players in room)",
                game_context_name(self.game),
                self.room_id,
                self.members.len()
            ),
            recent_messages: recent_chat_turns(&self.chat),
        };
        let tx = self.self_tx.clone();
        tokio::spawn(async move {
            let result = ai_friend.generate_reply(ctx).await;
            let _ = tx.send(LobbyCmd::AiReply { generation, result }).await;
        });
    }

    fn handle_ai_reply(&mut self, generation: u64, result: Result<String, PortError>) {
        let succeeded = result.is_ok();
        if !self
            .ai_chat
            .finish_request(generation, succeeded, Instant::now())
        {
            return;
        }
        match result {
            Ok(reply) => {
                if let Some(clean) = sanitize_ai_reply(&reply) {
                    self.push_chat_and_broadcast(make_ai_chat_line(clean));
                }
            }
            Err(PortError::Conflict(_)) => {
                self.ai_chat.set_enabled(false);
                self.broadcast_all(&ServerMessage::ChatAiState {
                    scope: ChatScopeDto::Lobby,
                    enabled: false,
                });
                self.push_system_chat("🤖 AI Friend is taking a break (API call limit reached).");
            }
            Err(_) => {}
        }
    }

    fn push_system_chat(&mut self, text: &str) {
        self.push_chat_and_broadcast(ChatLineDto {
            from: None,
            name: None,
            text: text.to_owned(),
            ts: Utc::now().timestamp_millis(),
            sys: true,
            ai: false,
        });
    }

    fn push_chat_and_broadcast(&mut self, line: ChatLineDto) {
        if self.chat.len() >= MAX_CHAT_HISTORY {
            let _ = self.chat.pop_front();
        }
        self.chat.push_back(line.clone());
        self.broadcast_all(&ServerMessage::Chat {
            scope: ChatScopeDto::Lobby,
            line,
        });
    }

    fn handle_create_table(&mut self, user_id: &str, settings: TableSettingsDto) {
        let Some(host_client) = self.build_table_client(user_id) else {
            return;
        };
        if self.hub.is_draining()
            || self.hub.is_abandon_cooldown(user_id)
            || self
                .members
                .get(user_id)
                .and_then(|m| m.active_table.as_ref())
                .is_some()
        {
            let _ = host_client.sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::NotAllowed,
                re: "table.create".to_owned(),
            });
            return;
        }
        self.play_now_queue.retain(|t| t.user_id != user_id);
        self.spawn_table_and_seat(host_client, None, None, settings);
    }

    fn handle_play_now(&mut self, user_id: &str, vs_bot: Option<BotLevelDto>) {
        let Some(caller) = self.build_table_client(user_id) else {
            return;
        };
        if self.hub.is_draining()
            || self.hub.is_abandon_cooldown(user_id)
            || self
                .members
                .get(user_id)
                .and_then(|m| m.active_table.as_ref())
                .is_some()
        {
            let _ = caller.sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::NotAllowed,
                re: "play.now".to_owned(),
            });
            return;
        }

        if let Some(level) = vs_bot {
            self.play_now_queue.retain(|t| t.user_id != user_id);
            let settings = TableSettingsDto {
                rated: false,
                ..TableSettingsDto::default()
            };
            self.spawn_table_and_seat(caller, None, Some(level), settings);
            return;
        }

        // Remove any prior ticket for this caller.
        self.play_now_queue.retain(|t| t.user_id != user_id);

        // Try immediate match against another queued human within Elo window.
        let now = Instant::now();
        if let Some(match_idx) = self.find_queue_match(caller.user.rating, now) {
            let partner_ticket = self.play_now_queue.remove(match_idx);
            if let Some(partner) = self.build_table_client(&partner_ticket.user_id) {
                let settings = TableSettingsDto {
                    rated: !caller.user.guest && !partner.user.guest,
                    ..TableSettingsDto::default()
                };
                self.spawn_table_and_seat(partner, Some(caller), None, settings);
                return;
            }
        }

        // Otherwise enqueue caller for human-first matchmaking (20s wait).
        let _ = caller.sink.send_msg(&ServerMessage::PlayNowQueued {
            ms_wait: u64::try_from(PLAY_NOW_WAIT_TIMEOUT.as_millis()).unwrap_or(20_000),
        });
        self.play_now_queue.push(PlayNowTicket {
            user_id: user_id.to_owned(),
            rating: caller.user.rating,
            guest: caller.user.guest,
            queued_at: now,
        });
    }

    fn handle_send_invite(
        &mut self,
        from_id: &str,
        to_id: &str,
        table_id: Option<String>,
        settings: Option<TableSettingsDto>,
    ) {
        if from_id == to_id {
            self.send_error(from_id, ErrorCode::Invalid, "invite.send");
            return;
        }
        let Some(inviter) = self.members.get(from_id) else {
            return;
        };
        let inviter_dto = inviter.user.clone();
        let Some(target) = self.members.get(to_id) else {
            self.send_error(from_id, ErrorCode::Invalid, "invite.send");
            return;
        };
        if target.ignored.contains(from_id) || target.user.status == UserStatusDto::Playing {
            self.send_error(from_id, ErrorCode::NotAllowed, "invite.send");
            return;
        }
        let target_sink = target.sink.clone();

        // Whitelist target on an existing table if inviting into one (fire-and-forget).
        if let Some(ref tid) = table_id
            && let Some(handle) = self.hub.find_table(tid)
        {
            handle.try_send(TableCmd::AllowInvite {
                user_id: to_id.to_owned(),
            });
        }

        let invite_id = self.hub.alloc_invite_id();
        let chosen_settings = settings.unwrap_or_default();
        self.invites.insert(
            invite_id.clone(),
            PendingInvite {
                id: invite_id.clone(),
                from_id: from_id.to_owned(),
                to_id: to_id.to_owned(),
                table_id: table_id.clone(),
                settings: chosen_settings.clone(),
                created_at: Instant::now(),
            },
        );
        let _ = target_sink.send_msg(&ServerMessage::InviteReceived {
            invite_id,
            from: inviter_dto,
            game: self.game,
            table_id,
            settings: chosen_settings,
            ms_left: u64::try_from(INVITE_TTL.as_millis()).unwrap_or(30_000),
        });
    }

    fn handle_answer_invite(&mut self, user_id: &str, invite_id: &str, accept: bool) {
        let Some(invite) = self.invites.remove(invite_id) else {
            self.send_error(user_id, ErrorCode::Invalid, "invite.answer");
            return;
        };
        if invite.to_id != user_id {
            self.invites.insert(invite.id.clone(), invite);
            self.send_error(user_id, ErrorCode::NotAllowed, "invite.answer");
            return;
        }

        if !accept {
            if let Some(inviter) = self.members.get(&invite.from_id) {
                let _ = inviter.sink.send_msg(&ServerMessage::InviteClosed {
                    invite_id: invite.id,
                    reason: InviteCloseReasonDto::Declined,
                });
            }
            return;
        }

        if let Some(tid) = invite.table_id {
            // Invite into an existing table: tell invitee's table via fire-and-forget? Better:
            // Spawn a small detached task to route Join into the table so LobbyActor NEVER awaits TableActor!
            if let Some(handle) = self.hub.find_table(&tid)
                && let Some(invitee_client) = self.build_table_client(user_id)
            {
                let sink = invitee_client.sink.clone();
                tokio::spawn(async move {
                    let (tx, rx) = oneshot::channel();
                    if handle
                        .send(TableCmd::Join {
                            client: invitee_client,
                            role: boomer_protocol::TableRoleDto::Player,
                            reply: tx,
                        })
                        .await
                        && let Ok(res) = rx.await
                    {
                        match res {
                            Ok(snap) => {
                                let _ =
                                    sink.send_msg(&ServerMessage::TableSnapshot(Box::new(snap)));
                            }
                            Err(code) => {
                                let _ = sink.send_msg(&ServerMessage::Error {
                                    code,
                                    re: "invite.answer".to_owned(),
                                });
                            }
                        }
                    }
                });
            }
            return;
        }

        // Create a fresh table with inviter (Seat 1) and recipient (Seat 2).
        let Some(inviter) = self.build_table_client(&invite.from_id) else {
            self.send_error(user_id, ErrorCode::Invalid, "invite.answer");
            return;
        };
        let Some(recipient) = self.build_table_client(user_id) else {
            return;
        };
        self.spawn_table_and_seat(inviter, Some(recipient), None, invite.settings);
    }

    fn on_tick(&mut self) {
        let now = Instant::now();

        // 1. Expire invites older than 30 seconds.
        let expired_ids: Vec<String> = self
            .invites
            .values()
            .filter(|inv| now.duration_since(inv.created_at) >= INVITE_TTL)
            .map(|inv| inv.id.clone())
            .collect();
        for id in expired_ids {
            if let Some(inv) = self.invites.remove(&id) {
                let msg = ServerMessage::InviteClosed {
                    invite_id: inv.id,
                    reason: InviteCloseReasonDto::Expired,
                };
                if let Some(m) = self.members.get(&inv.from_id) {
                    let _ = m.sink.send_msg(&msg);
                }
                if let Some(m) = self.members.get(&inv.to_id) {
                    let _ = m.sink.send_msg(&msg);
                }
            }
        }

        // 2. Match queued Play Now tickets as Elo windows widen over time.
        self.try_match_queued_pairs(now);

        // 3. Emit bot-suggestion timeout for tickets waiting >= 20 seconds.
        let mut timed_out = Vec::new();
        self.play_now_queue.retain(|ticket| {
            if now.duration_since(ticket.queued_at) >= PLAY_NOW_WAIT_TIMEOUT {
                timed_out.push(ticket.user_id.clone());
                false
            } else {
                true
            }
        });
        for uid in timed_out {
            if let Some(member) = self.members.get(&uid) {
                let _ = member
                    .sink
                    .send_msg(&ServerMessage::PlayNowTimeout { suggest_bot: true });
            }
        }
    }

    fn try_match_queued_pairs(&mut self, now: Instant) {
        while self.play_now_queue.len() >= 2 {
            let mut matched_pair: Option<(usize, usize)> = None;
            'outer: for first_idx in 0..self.play_now_queue.len() {
                let Some(first) = self.play_now_queue.get(first_idx) else {
                    continue;
                };
                for second_idx in (first_idx + 1)..self.play_now_queue.len() {
                    let Some(second) = self.play_now_queue.get(second_idx) else {
                        continue;
                    };
                    let max_diff =
                        Self::ticket_window(first, now).max(Self::ticket_window(second, now));
                    if (first.rating - second.rating).abs() <= max_diff {
                        matched_pair = Some((first_idx, second_idx));
                        break 'outer;
                    }
                }
            }
            let Some((first_idx, second_idx)) = matched_pair else {
                break;
            };
            let second_ticket = self.play_now_queue.remove(second_idx);
            let first_ticket = self.play_now_queue.remove(first_idx);
            if let (Some(first_client), Some(second_client)) = (
                self.build_table_client(&first_ticket.user_id),
                self.build_table_client(&second_ticket.user_id),
            ) {
                let settings = TableSettingsDto {
                    rated: !first_ticket.guest && !second_ticket.guest,
                    ..TableSettingsDto::default()
                };
                self.spawn_table_and_seat(first_client, Some(second_client), None, settings);
            }
        }
    }

    fn find_queue_match(&self, rating: i32, now: Instant) -> Option<usize> {
        self.play_now_queue.iter().position(|ticket| {
            let max_diff = Self::ticket_window(ticket, now);
            (ticket.rating - rating).abs() <= max_diff
        })
    }

    fn ticket_window(ticket: &PlayNowTicket, now: Instant) -> i32 {
        let elapsed_secs = now.duration_since(ticket.queued_at).as_secs();
        let steps = i32::try_from(elapsed_secs / 5).unwrap_or(10);
        PLAY_NOW_BASE_WINDOW + steps * PLAY_NOW_WIDEN_STEP
    }

    fn spawn_table_and_seat(
        &mut self,
        host: TableClient,
        second_human: Option<TableClient>,
        second_bot: Option<BotLevelDto>,
        settings: TableSettingsDto,
    ) {
        let no = self.hub.alloc_table_no();
        let id = format!("t{no}");
        let host_id = host.user.id.clone();
        let host_sink = host.sink.clone();
        let second_info = second_human
            .as_ref()
            .map(|c| (c.user.id.clone(), c.sink.clone()));

        let (_handle, summary, snapshot) = TableActor::spawn(
            TableInit {
                id: id.clone(),
                no,
                room_id: self.room_id.clone(),
                game: self.game,
                settings,
                host: Some(host),
                second_human,
                second_bot,
                is_house_bot: false,
            },
            self.event_tx.clone(),
            &self.hub,
        );

        self.tables.insert(id.clone(), summary.clone());
        self.broadcast_all(&ServerMessage::TableUpsert(Box::new(summary)));

        self.mark_member_at_table(&host_id, &id);
        let snap_msg = ServerMessage::TableSnapshot(Box::new(snapshot));
        let _ = host_sink.send_msg(&snap_msg);
        if let Some((second_id, second_sink)) = second_info {
            self.mark_member_at_table(&second_id, &id);
            let _ = second_sink.send_msg(&snap_msg);
        }
    }

    fn mark_member_at_table(&mut self, user_id: &str, table_id: &str) {
        if let Some(member) = self.members.get_mut(user_id) {
            member.user.status = UserStatusDto::Waiting;
            member.active_table = Some(table_id.to_owned());
            let dto = member.user.clone();
            self.broadcast_all(&ServerMessage::UserUpdate(dto));
        }
    }

    fn cancel_invites_for(&mut self, user_id: &str) {
        let ids: Vec<String> = self
            .invites
            .values()
            .filter(|inv| inv.from_id == user_id || inv.to_id == user_id)
            .map(|inv| inv.id.clone())
            .collect();
        for id in ids {
            if let Some(inv) = self.invites.remove(&id) {
                let other = if inv.from_id == user_id {
                    &inv.to_id
                } else {
                    &inv.from_id
                };
                if let Some(member) = self.members.get(other) {
                    let _ = member.sink.send_msg(&ServerMessage::InviteClosed {
                        invite_id: inv.id,
                        reason: InviteCloseReasonDto::Cancelled,
                    });
                }
            }
        }
    }

    fn build_table_client(&self, user_id: &str) -> Option<TableClient> {
        let member = self.members.get(user_id)?;
        Some(TableClient {
            user: member.user.clone(),
            sink: member.sink.clone(),
            last_chat_at: None,
            muted: member.muted,
        })
    }

    fn sorted_users(&self) -> Vec<UserDto> {
        let mut list: Vec<UserDto> = self.members.values().map(|m| m.user.clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    fn sorted_tables(&self) -> Vec<TableSummaryDto> {
        let mut list: Vec<TableSummaryDto> = self
            .tables
            .values()
            .filter(|t| {
                t.visibility != TableVisibilityDto::Private || t.phase != TablePhaseDto::Over
            })
            .cloned()
            .collect();
        list.sort_by_key(|t| t.no);
        list
    }

    fn broadcast_all(&self, msg: &ServerMessage) {
        broadcast_msg(self.members.values().map(|m| &m.sink), msg);
    }

    fn send_error(&self, user_id: &str, code: ErrorCode, re: &str) {
        if let Some(member) = self.members.get(user_id) {
            let _ = member.sink.send_msg(&ServerMessage::Error {
                code,
                re: re.to_owned(),
            });
        }
    }
}

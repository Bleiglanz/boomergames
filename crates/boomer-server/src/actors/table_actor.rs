//! Single-table actor (`tokio` task per table, zero shared locks) hosting
//! authoritative `boomer-domain` Checkers & 8-Ball Pool games, House Bots,
//! bounded `spawn_blocking` AI turns, and atomic Elo / High Score persistence.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use boomer_domain::Seat;
use boomer_domain::checkers::CheckersColor;
use boomer_domain::chess::ChessColor;
use boomer_domain::rating::is_provisional;
use boomer_protocol::{
    BotLevelDto, ChatLineDto, ChatScopeDto, ErrorCode, GameKindDto, GameOverDto, KickReasonDto,
    PoolShotDto, RatingChangeDto, SeatDto, ServerMessage, TablePhaseDto, TableRoleDto,
    TableSettingsDto, TableSnapshotDto, TableSummaryDto, TableUpdateDto, TableVisibilityDto,
    UserDto, UserStatusDto, WinCountDto,
};
use chrono::Utc;
use tokio::sync::{mpsc, oneshot};

use crate::actors::ai_chat::{
    AiChatState, game_context_name, make_ai_chat_line, recent_chat_turns, sanitize_ai_reply,
};
use crate::actors::game_session::{
    ActiveGame, BotDecision, BotWorkRequest, MatchFinishInfo, ServerCheckersMatch,
    ServerChessMatch, ServerPoolMatch, spawn_bot_turn,
};
use crate::actors::hub::LobbyHub;
use crate::actors::lobby_actor::LobbyEvent;
use crate::actors::{
    ClientSink, MAX_CHAT_HISTORY, bot_user_dto, broadcast_msg, prune_expired_chat,
    sanitize_chat_text,
};
use crate::mapping::{
    bot_level_to_db, format_user_id, game_kind_from_dto, parse_user_id, seat_to_dto,
};
use crate::ports::{AiChatContext, PortError, RecordMatchInput, RecordMatchOutcome};

/// Rejoin cooldown after being booted by the table host (5 minutes per Milestone 5.3).
const BOOT_BLOCK_DURATION: Duration = Duration::from_secs(5 * 60);

/// Table chat rate limit (1 message per second per `incubation/plan.md` §9.6).
const CHAT_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Bounded command mailbox capacity for each [`TableActor`].
const TABLE_CMD_CAPACITY: usize = 64;

/// Cloneable handle for sending downward commands to a [`TableActor`].
#[derive(Debug, Clone)]
pub struct TableHandle {
    /// Table ID (`"t1"`).
    pub table_id: String,
    /// Owning room ID.
    pub room_id: String,
    tx: mpsc::Sender<TableCmd>,
}

impl TableHandle {
    /// Sends a command to the table actor without awaiting a reply.
    pub fn try_send(&self, cmd: TableCmd) {
        let _ = self.tx.try_send(cmd);
    }

    /// Sends a command to the table actor (downward call from `WsConnection`).
    pub async fn send(&self, cmd: TableCmd) -> bool {
        self.tx.send(cmd).await.is_ok()
    }
}

/// Human client seated or spectating at a table.
#[derive(Debug, Clone)]
pub struct TableClient {
    /// Current user metadata.
    pub user: UserDto,
    /// Non-blocking outbound WebSocket handle.
    pub sink: ClientSink,
    /// Last table chat send timestamp for 1 msg/s rate limiting.
    pub last_chat_at: Option<Instant>,
    /// Whether this user is muted by moderation.
    pub muted: bool,
}

#[derive(Debug, Clone)]
enum SeatOccupant {
    Human(TableClient),
    Bot(BotLevelDto),
}

impl SeatOccupant {
    fn to_user_dto(&self, phase: TablePhaseDto) -> UserDto {
        let status = match phase {
            TablePhaseDto::Playing => UserStatusDto::Playing,
            TablePhaseDto::Waiting | TablePhaseDto::Over => UserStatusDto::Waiting,
        };
        match self {
            Self::Human(client) => {
                let mut dto = client.user.clone();
                dto.status = status;
                dto
            }
            Self::Bot(level) => bot_user_dto(*level, status),
        }
    }

    fn wire_id(&self) -> String {
        match self {
            Self::Human(client) => client.user.id.clone(),
            Self::Bot(level) => bot_user_dto(*level, UserStatusDto::Waiting).id,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Human(client) => client.user.name.clone(),
            Self::Bot(level) => bot_user_dto(*level, UserStatusDto::Waiting).name,
        }
    }

    fn bot_level(&self) -> Option<BotLevelDto> {
        match self {
            Self::Bot(level) => Some(*level),
            Self::Human(_) => None,
        }
    }

    const fn is_bot(&self) -> bool {
        matches!(self, Self::Bot(_))
    }
}

/// Initial state supplied by `LobbyActor` when spawning a new [`TableActor`].
pub struct TableInit {
    /// Table ID (`"t1"`).
    pub id: String,
    /// Sequential table number.
    pub no: u32,
    /// Owning room ID.
    pub room_id: String,
    /// Game kind.
    pub game: GameKindDto,
    /// Initial table settings.
    pub settings: TableSettingsDto,
    /// Seat 1 occupant (`None` for always-open 🤖 House Bot tables).
    pub host: Option<TableClient>,
    /// Optional Seat 2 human occupant (for `Play Now` or accepted invites).
    pub second_human: Option<TableClient>,
    /// Optional Seat 2 computer bot (for House Bot tables or `play.now { vsBot }`).
    pub second_bot: Option<BotLevelDto>,
    /// `true` for always-open room House Bot tables that stay open when empty.
    pub is_house_bot: bool,
}

/// Downward commands processed by a [`TableActor`].
pub enum TableCmd {
    /// Join this table as a player or spectator.
    Join {
        /// Joining client metadata and sink.
        client: TableClient,
        /// Desired role (`Player` or `Spectator`).
        role: TableRoleDto,
        /// Reply with full snapshot or protocol error.
        reply: oneshot::Sender<Result<TableSnapshotDto, ErrorCode>>,
    },
    /// Leave this table (returns to lobby or disconnects).
    Leave {
        /// Departing user wire ID.
        user_id: String,
    },
    /// Send a chat line to this table's isolated chat.
    SendChat {
        /// Sender wire ID.
        user_id: String,
        /// Raw message text.
        text: String,
    },
    /// Toggle the optional Claude AI Friend on or off for this table's chat.
    SetAiFriend {
        /// Requesting participant wire ID.
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
    /// Toggle Ready for a seated player.
    ToggleReady {
        /// Seated player wire ID.
        user_id: String,
    },
    /// Host updates pre-game table settings.
    UpdateSettings {
        /// Requesting user wire ID.
        user_id: String,
        /// Replacement table settings.
        settings: TableSettingsDto,
    },
    /// Spectator moves into an open seat.
    TakeSeat {
        /// Requesting spectator wire ID.
        user_id: String,
        /// Target seat (`1` or `2`).
        seat: SeatDto,
    },
    /// Seated player stands up into the spectator list.
    LeaveSeat {
        /// Seated player wire ID.
        user_id: String,
    },
    /// Host boots another user from the table (5-minute block).
    BootUser {
        /// Host wire ID.
        host_id: String,
        /// Target user wire ID.
        target_id: String,
    },
    /// Host adds a computer opponent to the open seat.
    AddBot {
        /// Host wire ID.
        host_id: String,
        /// Bot difficulty tier.
        level: BotLevelDto,
    },
    /// Host removes the seated computer opponent.
    RemoveBot {
        /// Host wire ID.
        host_id: String,
    },
    /// Whitelist an invited user for `Protected`/`Private` tables.
    AllowInvite {
        /// Invited user wire ID.
        user_id: String,
    },
    /// Refresh a member's [`UserDto`] (after login/rename/avatar update).
    UpdateMemberUser {
        /// Updated user info.
        user: UserDto,
        /// Updated mute state.
        muted: bool,
    },
    /// Authoritative Checkers move from a seated player.
    CheckersMove {
        /// Acting user wire ID.
        user_id: String,
        /// Expected turn counter.
        turn: u32,
        /// Move square path (`1..=32`).
        path: Vec<u8>,
    },
    /// Authoritative Chess UCI move from a seated player.
    ChessMove {
        /// Acting user wire ID.
        user_id: String,
        /// Expected turn counter.
        turn: u32,
        /// Move in UCI coordinate notation (`e2e4`, `e7e8q`).
        uci: String,
    },
    /// Authoritative Pool shot from a seated player.
    PoolShoot {
        /// Acting user wire ID.
        user_id: String,
        /// Shot parameters.
        shot: PoolShotDto,
    },
    /// Cue-ball placement during Pool ball-in-hand.
    PoolPlaceCue {
        /// Acting user wire ID.
        user_id: String,
        /// Target X coordinate in metres.
        x: f64,
        /// Target Y coordinate in metres.
        y: f64,
    },
    /// Resign the active match.
    Resign {
        /// Resigning user wire ID.
        user_id: String,
    },
    /// Offer a draw in Checkers.
    DrawOffer {
        /// Offering user wire ID.
        user_id: String,
    },
    /// Answer a pending draw offer in Checkers.
    DrawAnswer {
        /// Responding user wire ID.
        user_id: String,
        /// `true` to accept, `false` to decline.
        accept: bool,
    },
    /// Request a post-match rematch (alternates starting colors / breaker).
    Rematch {
        /// Seated player wire ID.
        user_id: String,
    },
    /// Internal callback delivering an AI move/shot computed on `spawn_blocking`.
    BotDecision {
        /// Turn counter when search started (stale results are ignored).
        turn: u32,
        /// Chosen move or shot.
        decision: BotDecision,
    },
    /// Internal callback after atomic `MatchRepo::record_finished_match` completes.
    MatchPersisted {
        /// Terminal finish info.
        finish: MatchFinishInfo,
        /// Persisted Elo and High Score outcome.
        outcome: RecordMatchOutcome,
    },
    /// Socket dropped: triggers 45s `PAUSED_DISCONNECT` grace period if seated in a running game.
    Disconnect {
        /// Disconnected user wire ID.
        user_id: String,
    },
    /// Socket reconnected within grace period: swaps sink, sends snapshot, and resumes match.
    Reconnect {
        /// Reconnecting user wire ID.
        user_id: String,
        /// Fresh non-blocking outbound sink.
        sink: ClientSink,
        /// Reply returning `(table_snapshot, optional_game_snapshot)`.
        reply: oneshot::Sender<Option<(TableSnapshotDto, Option<ServerMessage>)>>,
    },
    /// Moderator action: forcibly close this table and kick all participants.
    ForceClose,
    /// Exports recent table chat lines for a moderation report snapshot.
    DumpChat {
        /// Reply channel returning formatted chat transcript.
        reply: oneshot::Sender<String>,
    },
}

/// Authoritative actor owning a single table's state with zero shared locks.
pub struct TableActor {
    id: String,
    no: u32,
    room_id: String,
    game: GameKindDto,
    host_id: String,
    settings: TableSettingsDto,
    seats: [Option<SeatOccupant>; 2],
    ready: [bool; 2],
    rematch_ready: [bool; 2],
    watchers: HashMap<String, TableClient>,
    phase: TablePhaseDto,
    chat: VecDeque<ChatLineDto>,
    ai_chat: AiChatState,
    active_game: Option<ActiveGame>,
    bot_thinking: bool,
    match_index: u32,
    is_house_bot: bool,
    house_bot_level: Option<BotLevelDto>,
    disconnected_seat: Option<(usize, Instant)>,
    force_closed: bool,
    booted_until: HashMap<String, Instant>,
    invited_ids: HashSet<String>,
    self_tx: mpsc::Sender<TableCmd>,
    rx: mpsc::Receiver<TableCmd>,
    lobby_events: mpsc::Sender<LobbyEvent>,
    hub: Arc<LobbyHub>,
}

impl TableActor {
    /// Spawns a new [`TableActor`] task synchronously (never `.await`s) and returns its handle,
    /// initial [`TableSummaryDto`], and initial [`TableSnapshotDto`].
    #[must_use]
    pub fn spawn(
        init: TableInit,
        lobby_events: mpsc::Sender<LobbyEvent>,
        hub: &Arc<LobbyHub>,
    ) -> (TableHandle, TableSummaryDto, TableSnapshotDto) {
        let (tx, rx) = mpsc::channel(TABLE_CMD_CAPACITY);
        let host_id = init
            .host
            .as_ref()
            .map_or_else(String::new, |h| h.user.id.clone());

        let mut settings = init.settings;
        if init.is_house_bot
            || init.second_bot.is_some()
            || init.host.as_ref().is_some_and(|c| c.user.guest)
            || init.second_human.as_ref().is_some_and(|c| c.user.guest)
        {
            settings.rated = false;
        }

        let first_seat = init.host.map(|h| {
            hub.set_user_table(&h.user.id, &init.id);
            SeatOccupant::Human(h)
        });
        let second_seat = if let Some(human) = init.second_human {
            hub.set_user_table(&human.user.id, &init.id);
            Some(SeatOccupant::Human(human))
        } else {
            init.second_bot.map(SeatOccupant::Bot)
        };

        let initial_ready = [
            false,
            second_seat.as_ref().is_some_and(SeatOccupant::is_bot),
        ];

        let actor = Self {
            id: init.id.clone(),
            no: init.no,
            room_id: init.room_id.clone(),
            game: init.game,
            host_id,
            settings,
            seats: [first_seat, second_seat],
            ready: initial_ready,
            rematch_ready: [false, false],
            watchers: HashMap::new(),
            phase: TablePhaseDto::Waiting,
            chat: VecDeque::with_capacity(MAX_CHAT_HISTORY),
            // Individual tables start with AI Friend OFF until toggled on.
            ai_chat: AiChatState::new(false),
            active_game: None,
            bot_thinking: false,
            match_index: 0,
            is_house_bot: init.is_house_bot,
            house_bot_level: if init.is_house_bot {
                init.second_bot
            } else {
                None
            },
            disconnected_seat: None,
            force_closed: false,
            booted_until: HashMap::new(),
            invited_ids: HashSet::new(),
            self_tx: tx.clone(),
            rx,
            lobby_events,
            hub: Arc::clone(hub),
        };

        let summary = actor.summary();
        let snapshot = actor.snapshot();
        let handle = TableHandle {
            table_id: init.id,
            room_id: init.room_id,
            tx,
        };
        hub.register_table(handle.clone());
        tokio::spawn(actor.run());
        (handle, summary, snapshot)
    }

    async fn run(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        let mut subticks: u8 = 0;

        loop {
            tokio::select! {
                maybe_cmd = self.rx.recv() => {
                    let Some(cmd) = maybe_cmd else {
                        break;
                    };
                    if !self.handle_cmd(cmd) {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    self.check_disconnect_expiry();
                    subticks = subticks.wrapping_add(1);
                    if subticks.is_multiple_of(4) {
                        self.on_tick_one_sec();
                    }
                }
            }
        }
        self.shutdown_cleanup();
    }

    fn handle_cmd(&mut self, cmd: TableCmd) -> bool {
        match cmd {
            TableCmd::Join {
                client,
                role,
                reply,
            } => {
                let res = self.handle_join(client, role);
                let _ = reply.send(res);
            }
            TableCmd::Leave { user_id } => self.handle_leave(&user_id),
            TableCmd::SendChat { user_id, text } => self.handle_chat(&user_id, &text),
            TableCmd::SetAiFriend { user_id, enabled } => {
                self.handle_set_ai_friend(&user_id, enabled);
            }
            TableCmd::AiReply { generation, result } => {
                self.handle_ai_reply(generation, result);
            }
            TableCmd::ToggleReady { user_id } => self.handle_ready(&user_id),
            TableCmd::UpdateSettings { user_id, settings } => {
                self.handle_update_settings(&user_id, settings);
            }
            TableCmd::TakeSeat { user_id, seat } => self.handle_take_seat(&user_id, seat),
            TableCmd::LeaveSeat { user_id } => self.handle_leave_seat(&user_id),
            TableCmd::BootUser { host_id, target_id } => self.handle_boot(&host_id, &target_id),
            TableCmd::AddBot { host_id, level } => self.handle_add_bot(&host_id, level),
            TableCmd::RemoveBot { host_id } => self.handle_remove_bot(&host_id),
            TableCmd::AllowInvite { user_id } => {
                self.invited_ids.insert(user_id);
            }
            TableCmd::UpdateMemberUser { user, muted } => {
                self.handle_update_member_user(user, muted);
            }
            TableCmd::CheckersMove {
                user_id,
                turn,
                path,
            } => self.handle_checkers_move(&user_id, turn, &path),
            TableCmd::ChessMove { user_id, turn, uci } => {
                self.handle_chess_move(&user_id, turn, &uci);
            }
            TableCmd::PoolShoot { user_id, shot } => self.handle_pool_shoot(&user_id, shot),
            TableCmd::PoolPlaceCue { user_id, x, y } => self.handle_pool_place_cue(&user_id, x, y),
            TableCmd::Resign { user_id } => self.handle_resign(&user_id),
            TableCmd::DrawOffer { user_id } => self.handle_draw_offer(&user_id),
            TableCmd::DrawAnswer { user_id, accept } => self.handle_draw_answer(&user_id, accept),
            TableCmd::Rematch { user_id } => self.handle_rematch(&user_id),
            TableCmd::BotDecision { turn, decision } => self.handle_bot_decision(turn, decision),
            TableCmd::MatchPersisted { finish, outcome } => {
                self.handle_match_persisted(&finish, &outcome);
            }
            TableCmd::Disconnect { user_id } => self.handle_disconnect(&user_id),
            TableCmd::Reconnect {
                user_id,
                sink,
                reply,
            } => {
                let res = self.handle_reconnect(&user_id, sink);
                let _ = reply.send(res);
            }
            TableCmd::ForceClose => {
                self.force_closed = true;
                return false;
            }
            TableCmd::DumpChat { reply } => {
                self.prune_expired_chat();
                let transcript = self
                    .chat
                    .iter()
                    .map(|l| format!("{}: {}", l.name.as_deref().unwrap_or("system"), l.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = reply.send(transcript);
            }
        }
        !self.force_closed && (self.is_house_bot || self.human_count() > 0)
    }

    fn check_disconnect_expiry(&mut self) {
        let Some((seat_idx, since)) = self.disconnected_seat else {
            return;
        };
        if since.elapsed() < self.hub.disconnect_grace() {
            return;
        }
        self.disconnected_seat = None;
        let loser_wire_id = self
            .seats
            .get(seat_idx)
            .and_then(|s| s.as_ref())
            .map(SeatOccupant::wire_id);
        let finish = {
            let Some(game) = self.active_game.as_mut() else {
                return;
            };
            if game.resign(seat_idx).is_err() {
                return;
            }
            game.finish_info().map(|mut info| {
                "disconnect".clone_into(&mut info.reason);
                info
            })
        };
        if let Some(info) = finish {
            self.push_system_chat("Disconnected player did not return in time (forfeit).");
            self.on_game_finished(info);
        }
        if let Some(uid) = loser_wire_id {
            self.hub.record_abandonment(&uid);
            self.hub.clear_user_table(&uid, &self.id);
            if let Some(slot) = self.seats.get_mut(seat_idx) {
                *slot = None;
            }
            self.emit_presence(&uid, UserStatusDto::Available, None);
        }
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_disconnect(&mut self, user_id: &str) {
        if self.phase == TablePhaseDto::Playing
            && let Some(seat_idx) = self.find_human_seat(user_id)
        {
            self.disconnected_seat = Some((seat_idx, Instant::now()));
            let seat_dto = if seat_idx == 0 {
                SeatDto::One
            } else {
                SeatDto::Two
            };
            let ms_left = u64::try_from(self.hub.disconnect_grace().as_millis()).unwrap_or(45_000);
            self.push_system_chat("Player disconnected — waiting 45s for reconnect…");
            self.broadcast_table_msg(&ServerMessage::GamePause {
                seat: seat_dto,
                ms_left,
            });
            return;
        }
        self.handle_leave(user_id);
    }

    fn handle_reconnect(
        &mut self,
        user_id: &str,
        sink: ClientSink,
    ) -> Option<(TableSnapshotDto, Option<ServerMessage>)> {
        let mut found_seat: Option<usize> = None;
        for (idx, occ) in self.seats.iter_mut().enumerate() {
            if let Some(SeatOccupant::Human(client)) = occ
                && client.user.id == user_id
            {
                client.sink = sink.clone();
                found_seat = Some(idx);
                break;
            }
        }
        if found_seat.is_none()
            && let Some(watcher) = self.watchers.get_mut(user_id)
        {
            watcher.sink = sink;
        } else if found_seat.is_none() {
            return None;
        }

        if let Some(seat_idx) = found_seat
            && self.disconnected_seat.is_some_and(|(s, _)| s == seat_idx)
        {
            self.disconnected_seat = None;
            self.push_system_chat("Player reconnected — match resumed.");
            self.broadcast_table_msg(&ServerMessage::GameResume {});
        }

        self.prune_expired_chat();
        let game_snap = self
            .active_game
            .as_ref()
            .map(|g| g.snapshot_message(self.bot_thinking));
        Some((self.snapshot(), game_snap))
    }

    fn on_tick_one_sec(&mut self) {
        self.prune_expired_chat();
        if self.phase != TablePhaseDto::Playing || self.disconnected_seat.is_some() {
            return;
        }
        let (snap, finish) = {
            let Some(game) = self.active_game.as_mut() else {
                return;
            };
            if !game.tick_one_sec() {
                return;
            }
            (game.snapshot_message(self.bot_thinking), game.finish_info())
        };
        self.broadcast_table_msg(&snap);
        if let Some(info) = finish {
            self.on_game_finished(info);
        } else {
            self.maybe_schedule_bot_turn();
        }
    }

    fn handle_join(
        &mut self,
        client: TableClient,
        role: TableRoleDto,
    ) -> Result<TableSnapshotDto, ErrorCode> {
        let uid = client.user.id.clone();

        if let Some(&until) = self.booted_until.get(&uid)
            && Instant::now() < until
        {
            return Err(ErrorCode::NotAllowed);
        }

        let is_invited = self.invited_ids.contains(&uid) || uid == self.host_id;
        if self.settings.visibility == TableVisibilityDto::Private && !is_invited {
            return Err(ErrorCode::NotAllowed);
        }
        if role == TableRoleDto::Player
            && self.settings.visibility == TableVisibilityDto::Protected
            && !is_invited
        {
            return Err(ErrorCode::NotAllowed);
        }

        if self.find_human_seat(&uid).is_some() || self.watchers.contains_key(&uid) {
            self.prune_expired_chat();
            let snap = self.snapshot();
            if let Some(ref game) = self.active_game {
                let _ = client
                    .sink
                    .send_msg(&game.snapshot_message(self.bot_thinking));
            }
            return Ok(snap);
        }

        let join_sink = client.sink.clone();
        let new_status = match role {
            TableRoleDto::Player => {
                let open_idx = self.first_open_seat_index().ok_or(ErrorCode::TableFull)?;
                if client.user.guest {
                    self.settings.rated = false;
                }
                if self.host_id.is_empty() {
                    self.host_id.clone_from(&uid);
                }
                if let Some(slot) = self.seats.get_mut(open_idx) {
                    *slot = Some(SeatOccupant::Human(client));
                }
                if let Some(flag) = self.ready.get_mut(open_idx) {
                    *flag = false;
                }
                if self.phase == TablePhaseDto::Playing {
                    UserStatusDto::Playing
                } else {
                    UserStatusDto::Waiting
                }
            }
            TableRoleDto::Spectator => {
                self.watchers.insert(uid.clone(), client);
                UserStatusDto::Watching
            }
        };

        self.hub.set_user_table(&uid, &self.id);
        self.prune_expired_chat();
        let snap = self.snapshot();
        if let Some(ref game) = self.active_game {
            let _ = join_sink.send_msg(&game.snapshot_message(self.bot_thinking));
        }
        self.broadcast_update();
        self.emit_presence(&uid, new_status, Some(self.id.clone()));
        self.emit_summary();
        Ok(snap)
    }

    fn handle_leave(&mut self, user_id: &str) {
        let mut removed = false;
        if let Some(idx) = self.find_human_seat(user_id) {
            // Leaving mid-game counts as resignation if opponent is still seated.
            if self.phase == TablePhaseDto::Playing
                && let Some(game) = self.active_game.as_mut()
                && game.resign(idx).is_ok()
                && let Some(finish) = game.finish_info()
            {
                let snap = game.snapshot_message(false);
                self.broadcast_table_msg(&snap);
                self.on_game_finished(finish);
            }
            if let Some(slot) = self.seats.get_mut(idx) {
                *slot = None;
            }
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = false;
            }
            if let Some(flag) = self.rematch_ready.get_mut(idx) {
                *flag = false;
            }
            removed = true;
        } else if self.watchers.remove(user_id).is_some() {
            removed = true;
        }

        if !removed {
            return;
        }

        self.hub.clear_user_table(user_id, &self.id);
        self.emit_presence(user_id, UserStatusDto::Available, None);

        if self.host_id == user_id {
            self.promote_next_host();
        }

        if self.human_count() == 0 && self.is_house_bot {
            self.reset_house_bot_table();
            return;
        }

        if self.human_count() > 0 {
            self.broadcast_update();
            self.emit_summary();
        }
    }

    fn reset_house_bot_table(&mut self) {
        if self.phase == TablePhaseDto::Playing {
            self.hub.dec_games();
        }
        self.phase = TablePhaseDto::Waiting;
        self.active_game = None;
        self.bot_thinking = false;
        self.host_id.clear();
        self.seats = [None, self.house_bot_level.map(SeatOccupant::Bot)];
        self.ready = [false, self.house_bot_level.is_some()];
        self.rematch_ready = [false, false];
        self.chat.clear();
        self.ai_chat.reset(false);
        self.emit_summary();
    }

    fn handle_chat(&mut self, user_id: &str, raw_text: &str) {
        let is_watcher = self.watchers.contains_key(user_id);
        let mute_spectators = self.settings.mute_spectators;
        let Some(client) = self.find_client_mut(user_id) else {
            return;
        };
        let sink = client.sink.clone();

        if client.muted || (is_watcher && mute_spectators) {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::Muted,
                re: "chat.send".to_owned(),
            });
            return;
        }
        let now = Instant::now();
        if let Some(last) = client.last_chat_at
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
        client.last_chat_at = Some(now);
        let sender_name = client.user.name.clone();

        let line = ChatLineDto {
            from: Some(user_id.to_owned()),
            name: Some(sender_name),
            text: clean,
            ts: Utc::now().timestamp_millis(),
            sys: false,
            ai: false,
        };
        self.push_chat_and_broadcast(line);
        self.maybe_trigger_ai_reply();
    }

    fn handle_set_ai_friend(&mut self, user_id: &str, enabled: bool) {
        let is_watcher = self.watchers.contains_key(user_id);
        let mute_spectators = self.settings.mute_spectators;
        let Some(client) = self.find_client_mut(user_id) else {
            return;
        };
        let sink = client.sink.clone();
        let actor_name = client.user.name.clone();
        if client.muted || (is_watcher && mute_spectators) {
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
            self.broadcast_table_msg(&ServerMessage::ChatAiState {
                scope: ChatScopeDto::Table,
                enabled,
            });
            let state_word = if enabled { "on" } else { "off" };
            self.push_system_chat(&format!("{actor_name} turned AI Friend {state_word}."));
        } else {
            let _ = sink.send_msg(&ServerMessage::ChatAiState {
                scope: ChatScopeDto::Table,
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
        let solo_human = self.human_count() <= 1;
        let Some(generation) = self
            .ai_chat
            .on_human_message(Instant::now(), cooldown, solo_human)
        else {
            return;
        };
        let s1 = self.seats[0]
            .as_ref()
            .map_or_else(|| "open seat".to_owned(), SeatOccupant::label);
        let s2 = self.seats[1]
            .as_ref()
            .map_or_else(|| "open seat".to_owned(), SeatOccupant::label);
        let phase_str = match self.phase {
            TablePhaseDto::Waiting => "waiting to start",
            TablePhaseDto::Playing => "match in progress",
            TablePhaseDto::Over => "match finished",
        };
        let turn_info = self
            .active_game
            .as_ref()
            .map_or_else(String::new, |g| format!(", turn {}", g.turn_number()));
        self.prune_expired_chat();
        let ctx = AiChatContext {
            setting: format!(
                "Table #{} ({}, {}: {} vs {}{}, {} spectators)",
                self.no,
                game_context_name(self.game),
                phase_str,
                s1,
                s2,
                turn_info,
                self.watchers.len()
            ),
            recent_messages: recent_chat_turns(&self.chat),
        };
        let tx = self.self_tx.clone();
        tokio::spawn(async move {
            let result = ai_friend.generate_reply(ctx).await;
            let _ = tx.send(TableCmd::AiReply { generation, result }).await;
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
                self.broadcast_table_msg(&ServerMessage::ChatAiState {
                    scope: ChatScopeDto::Table,
                    enabled: false,
                });
                self.push_system_chat("🤖 AI Friend is taking a break (API call limit reached).");
            }
            Err(_) => {}
        }
    }

    fn handle_ready(&mut self, user_id: &str) {
        let Some(idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "ready");
            return;
        };
        if self.phase != TablePhaseDto::Waiting || self.hub.is_draining() {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "ready");
            return;
        }
        if let Some(flag) = self.ready.get_mut(idx) {
            *flag = !*flag;
        }
        let both_occupied = self.seats[0].is_some() && self.seats[1].is_some();
        if both_occupied && self.ready == [true, true] {
            self.start_new_match();
            return;
        }
        self.broadcast_update();
        self.emit_summary();
    }

    fn start_new_match(&mut self) {
        self.phase = TablePhaseDto::Playing;
        self.rematch_ready = [false, false];
        self.bot_thinking = false;
        self.hub.inc_games();

        let swap = self.match_index % 2 == 1;
        self.match_index = self.match_index.saturating_add(1);

        let engine = match self.game {
            GameKindDto::Checkers => {
                let colors = if swap {
                    [CheckersColor::White, CheckersColor::Black]
                } else {
                    [CheckersColor::Black, CheckersColor::White]
                };
                ActiveGame::Checkers(ServerCheckersMatch::new(colors, &self.settings))
            }
            GameKindDto::Pool => {
                let breaker = if swap { Seat::Two } else { Seat::One };
                let seed = self.hub.alloc_seed();
                ActiveGame::Pool(Box::new(ServerPoolMatch::new(
                    breaker,
                    &self.settings,
                    seed,
                )))
            }
            GameKindDto::Chess => {
                let colors = if swap {
                    [ChessColor::Black, ChessColor::White]
                } else {
                    [ChessColor::White, ChessColor::Black]
                };
                ActiveGame::Chess(Box::new(ServerChessMatch::new(colors, &self.settings)))
            }
        };

        let initial_snap = engine.snapshot_message(false);
        self.active_game = Some(engine);
        self.push_system_chat("Match started!");
        for occ in self.seats.iter().flatten() {
            if let SeatOccupant::Human(c) = occ {
                self.emit_presence(&c.user.id, UserStatusDto::Playing, Some(self.id.clone()));
            }
        }
        self.broadcast_update();
        self.broadcast_table_msg(&initial_snap);
        self.emit_summary();
        self.maybe_schedule_bot_turn();
    }

    fn handle_checkers_move(&mut self, user_id: &str, turn: u32, path: &[u8]) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotYourTurn, "checkers.move");
            return;
        };
        if self.phase != TablePhaseDto::Playing {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "checkers.move");
            return;
        }
        let Some(ActiveGame::Checkers(match_state)) = self.active_game.as_mut() else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "checkers.move");
            return;
        };
        if let Err(code) = match_state.apply_move(seat_idx, turn, path) {
            self.send_error_to(user_id, code, "checkers.move");
            return;
        }
        let finish = match_state.finish_info();
        let snap = ServerMessage::CheckersSnapshot {
            state: match_state.to_dto(false),
        };
        self.broadcast_table_msg(&snap);
        if let Some(info) = finish {
            self.on_game_finished(info);
        } else {
            self.maybe_schedule_bot_turn();
        }
    }

    fn handle_chess_move(&mut self, user_id: &str, turn: u32, uci: &str) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotYourTurn, "chess.move");
            return;
        };
        if self.phase != TablePhaseDto::Playing {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "chess.move");
            return;
        }
        let Some(ActiveGame::Chess(match_state)) = self.active_game.as_mut() else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "chess.move");
            return;
        };
        if let Err(code) = match_state.apply_move(seat_idx, turn, uci) {
            self.send_error_to(user_id, code, "chess.move");
            return;
        }
        let finish = match_state.finish_info();
        let snap = ServerMessage::ChessSnapshot {
            state: match_state.to_dto(false),
        };
        self.broadcast_table_msg(&snap);
        if let Some(info) = finish {
            self.on_game_finished(info);
        } else {
            self.maybe_schedule_bot_turn();
        }
    }

    fn handle_pool_place_cue(&mut self, user_id: &str, x: f64, y: f64) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotYourTurn, "pool.place_cue");
            return;
        };
        if self.phase != TablePhaseDto::Playing {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "pool.place_cue");
            return;
        }
        let Some(ActiveGame::Pool(match_state)) = self.active_game.as_mut() else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "pool.place_cue");
            return;
        };
        if let Err(code) = match_state.place_cue(seat_idx, x, y) {
            self.send_error_to(user_id, code, "pool.place_cue");
            return;
        }
        let snap = ServerMessage::PoolSnapshot {
            state: match_state.to_dto(false),
        };
        self.broadcast_table_msg(&snap);
    }

    fn handle_pool_shoot(&mut self, user_id: &str, shot: PoolShotDto) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotYourTurn, "pool.shoot");
            return;
        };
        if self.phase != TablePhaseDto::Playing {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "pool.shoot");
            return;
        }
        let Some(ActiveGame::Pool(match_state)) = self.active_game.as_mut() else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "pool.shoot");
            return;
        };
        let (pre_balls, applied_shot, post_state) = match match_state.shoot(seat_idx, shot) {
            Ok(res) => res,
            Err(code) => {
                self.send_error_to(user_id, code, "pool.shoot");
                return;
            }
        };
        let finish = match_state.finish_info();
        self.broadcast_table_msg(&ServerMessage::PoolShot {
            pre_balls,
            shot: applied_shot,
            state: post_state,
        });
        if let Some(info) = finish {
            self.on_game_finished(info);
        } else {
            self.maybe_schedule_bot_turn();
        }
    }

    fn handle_resign(&mut self, user_id: &str) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "resign");
            return;
        };
        if self.phase != TablePhaseDto::Playing {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "resign");
            return;
        }
        let Some(game) = self.active_game.as_mut() else {
            return;
        };
        if let Err(code) = game.resign(seat_idx) {
            self.send_error_to(user_id, code, "resign");
            return;
        }
        let snap = game.snapshot_message(false);
        let finish = game.finish_info();
        self.broadcast_table_msg(&snap);
        if let Some(info) = finish {
            self.on_game_finished(info);
        }
    }

    fn handle_draw_offer(&mut self, user_id: &str) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "draw.offer");
            return;
        };
        let opp_is_bot = self
            .seats
            .get(1 - seat_idx)
            .is_some_and(|s| s.as_ref().is_some_and(SeatOccupant::is_bot));
        let seat = if seat_idx == 0 { Seat::One } else { Seat::Two };
        let snap = match self.active_game.as_mut() {
            Some(ActiveGame::Checkers(match_state)) => {
                if let Err(code) = match_state.offer_draw(seat) {
                    self.send_error_to(user_id, code, "draw.offer");
                    return;
                }
                if opp_is_bot {
                    let _ = match_state.answer_draw(seat.opponent(), false);
                }
                ServerMessage::CheckersSnapshot {
                    state: match_state.to_dto(self.bot_thinking),
                }
            }
            Some(ActiveGame::Chess(match_state)) => {
                if let Err(code) = match_state.offer_draw(seat) {
                    self.send_error_to(user_id, code, "draw.offer");
                    return;
                }
                if opp_is_bot {
                    let _ = match_state.answer_draw(seat.opponent(), false);
                }
                ServerMessage::ChessSnapshot {
                    state: match_state.to_dto(self.bot_thinking),
                }
            }
            _ => {
                self.send_error_to(user_id, ErrorCode::NotAllowed, "draw.offer");
                return;
            }
        };
        if opp_is_bot {
            self.push_system_chat("Computer declined the draw offer.");
        }
        self.broadcast_table_msg(&snap);
    }

    fn handle_draw_answer(&mut self, user_id: &str, accept: bool) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "draw.answer");
            return;
        };
        let seat = if seat_idx == 0 { Seat::One } else { Seat::Two };
        let (finish, snap) = match self.active_game.as_mut() {
            Some(ActiveGame::Checkers(match_state)) => {
                if let Err(code) = match_state.answer_draw(seat, accept) {
                    self.send_error_to(user_id, code, "draw.answer");
                    return;
                }
                (
                    match_state.finish_info(),
                    ServerMessage::CheckersSnapshot {
                        state: match_state.to_dto(false),
                    },
                )
            }
            Some(ActiveGame::Chess(match_state)) => {
                if let Err(code) = match_state.answer_draw(seat, accept) {
                    self.send_error_to(user_id, code, "draw.answer");
                    return;
                }
                (
                    match_state.finish_info(),
                    ServerMessage::ChessSnapshot {
                        state: match_state.to_dto(false),
                    },
                )
            }
            _ => {
                self.send_error_to(user_id, ErrorCode::NotAllowed, "draw.answer");
                return;
            }
        };
        self.broadcast_table_msg(&snap);
        if let Some(info) = finish {
            self.on_game_finished(info);
        }
    }

    fn handle_rematch(&mut self, user_id: &str) {
        let Some(seat_idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "rematch");
            return;
        };
        if self.phase != TablePhaseDto::Over {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "rematch");
            return;
        }
        if let Some(flag) = self.rematch_ready.get_mut(seat_idx) {
            *flag = true;
        }
        for (idx, occ) in self.seats.iter().enumerate() {
            if occ.as_ref().is_some_and(SeatOccupant::is_bot)
                && let Some(flag) = self.rematch_ready.get_mut(idx)
            {
                *flag = true;
            }
        }
        self.broadcast_table_msg(&ServerMessage::RematchState {
            ready: self.rematch_ready,
        });
        if self.seats[0].is_some() && self.seats[1].is_some() && self.rematch_ready == [true, true]
        {
            self.start_new_match();
        }
    }

    fn maybe_schedule_bot_turn(&mut self) {
        if self.phase != TablePhaseDto::Playing || self.bot_thinking {
            return;
        }
        let Some(game) = self.active_game.as_ref() else {
            return;
        };
        let active_idx = game.active_seat_index();
        let Some(level) = self
            .seats
            .get(active_idx)
            .and_then(|slot| slot.as_ref())
            .and_then(SeatOccupant::bot_level)
        else {
            return;
        };

        self.bot_thinking = true;
        let turn = game.turn_number();
        let seed = self.hub.alloc_seed();
        let req = match game {
            ActiveGame::Checkers(m) => BotWorkRequest::Checkers {
                board: *m.game.board(),
                level,
                seed,
            },
            ActiveGame::Pool(m) => BotWorkRequest::Pool {
                game: Box::new(m.game.clone()),
                level,
                seed,
            },
            ActiveGame::Chess(m) => BotWorkRequest::Chess {
                position: Box::new(m.game.position().clone()),
                level,
                seed,
            },
        };
        spawn_bot_turn(self.hub.bot_semaphore(), self.self_tx.clone(), turn, req);
    }

    fn handle_bot_decision(&mut self, turn: u32, decision: BotDecision) {
        self.bot_thinking = false;
        if self.phase != TablePhaseDto::Playing {
            return;
        }
        let Some(game) = self.active_game.as_mut() else {
            return;
        };
        if game.turn_number() != turn {
            return;
        }
        let active_idx = game.active_seat_index();
        let is_bot_seat = self
            .seats
            .get(active_idx)
            .and_then(|s| s.as_ref())
            .is_some_and(SeatOccupant::is_bot);
        if !is_bot_seat {
            return;
        }

        let finish = match (game, decision) {
            (ActiveGame::Checkers(m), BotDecision::CheckersMove(path)) => {
                if m.apply_move(active_idx, turn, &path).is_err() {
                    return;
                }
                let snap = ServerMessage::CheckersSnapshot {
                    state: m.to_dto(false),
                };
                let info = m.finish_info();
                self.broadcast_table_msg(&snap);
                info
            }
            (ActiveGame::Pool(m), BotDecision::PoolShot(shot)) => {
                let Ok((pre_balls, applied, post_state)) = m.shoot(active_idx, shot) else {
                    return;
                };
                let info = m.finish_info();
                self.broadcast_table_msg(&ServerMessage::PoolShot {
                    pre_balls,
                    shot: applied,
                    state: post_state,
                });
                info
            }
            (ActiveGame::Chess(m), BotDecision::ChessMove(uci)) => {
                if m.apply_move(active_idx, turn, &uci).is_err() {
                    return;
                }
                let snap = ServerMessage::ChessSnapshot {
                    state: m.to_dto(false),
                };
                let info = m.finish_info();
                self.broadcast_table_msg(&snap);
                info
            }
            _ => return,
        };

        if let Some(info) = finish {
            self.on_game_finished(info);
        } else {
            self.maybe_schedule_bot_turn();
        }
    }

    fn on_game_finished(&mut self, finish: MatchFinishInfo) {
        if self.phase != TablePhaseDto::Playing {
            return;
        }
        self.phase = TablePhaseDto::Over;
        self.bot_thinking = false;
        self.rematch_ready = [false, false];
        self.hub.dec_games();

        for occ in self.seats.iter().flatten() {
            if let SeatOccupant::Human(c) = occ {
                self.emit_presence(&c.user.id, UserStatusDto::Waiting, Some(self.id.clone()));
            }
        }
        self.broadcast_update();
        self.emit_summary();

        let bot_level = self
            .seats
            .iter()
            .flatten()
            .find_map(SeatOccupant::bot_level)
            .map(bot_level_to_db);

        let p1_id = self.seats[0]
            .as_ref()
            .and_then(|s| parse_user_id(&s.wire_id()));
        let p2_id = self.seats[1]
            .as_ref()
            .and_then(|s| parse_user_id(&s.wire_id()));
        let p1_label = self.seats[0]
            .as_ref()
            .map_or_else(|| "Player 1".to_owned(), SeatOccupant::label);
        let p2_label = self.seats[1]
            .as_ref()
            .map_or_else(|| "Player 2".to_owned(), SeatOccupant::label);

        let input = RecordMatchInput {
            game: game_kind_from_dto(self.game),
            player1_id: p1_id,
            player2_id: p2_id,
            player1_label: p1_label,
            player2_label: p2_label,
            bot_level,
            winner: finish.winner,
            reason: finish.reason.clone(),
            rated_requested: self.settings.rated,
            activity_count: finish.activity_count,
        };

        let repo = self.hub.matches_repo();
        let tx = self.self_tx.clone();
        tokio::spawn(async move {
            let outcome = repo.record_finished_match(input).await.unwrap_or_default();
            let _ = tx.send(TableCmd::MatchPersisted { finish, outcome }).await;
        });
    }

    fn handle_match_persisted(&mut self, finish: &MatchFinishInfo, outcome: &RecordMatchOutcome) {
        let mut rating_dtos = Vec::with_capacity(outcome.rating_changes.len());
        for rc in &outcome.rating_changes {
            let wire_id = format_user_id(rc.user_id);
            rating_dtos.push(RatingChangeDto {
                id: wire_id.clone(),
                old: rc.old_rating,
                new: rc.new_rating,
            });
            let prov = is_provisional(rc.new_rated_games);
            let mut display_name = wire_id.clone();
            for occ in self.seats.iter_mut().flatten() {
                if let SeatOccupant::Human(c) = occ
                    && c.user.id == wire_id
                {
                    c.user.rating = rc.new_rating;
                    c.user.prov = prov;
                    display_name.clone_from(&c.user.name);
                }
            }
            self.push_system_chat(&format!(
                "{display_name}'s old rating: {}; new rating: {}",
                rc.old_rating, rc.new_rating
            ));
            let _ = self.lobby_events.try_send(LobbyEvent::UserRatingUpdated {
                user_id: wire_id,
                rating: rc.new_rating,
                prov,
            });
        }

        let win_dtos: Vec<WinCountDto> = outcome
            .win_counts
            .iter()
            .map(|w| WinCountDto {
                id: format_user_id(w.user_id),
                wins: w.wins,
            })
            .collect();

        self.broadcast_table_msg(&ServerMessage::GameOver(Box::new(GameOverDto {
            winner: finish.winner_seat,
            reason: finish.reason.clone(),
            rating: rating_dtos,
            wins: win_dtos,
        })));
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_update_settings(&mut self, user_id: &str, mut settings: TableSettingsDto) {
        if self.host_id != user_id {
            self.send_error_to(user_id, ErrorCode::NotHost, "settings.set");
            return;
        }
        if self.phase != TablePhaseDto::Waiting {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "settings.set");
            return;
        }
        let has_guest_or_bot = self.is_house_bot
            || self.seats.iter().flatten().any(|occ| match occ {
                SeatOccupant::Bot(_) => true,
                SeatOccupant::Human(c) => c.user.guest,
            });
        if has_guest_or_bot {
            settings.rated = false;
        }
        self.settings = settings;
        for (idx, occ) in self.seats.iter().enumerate() {
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = matches!(occ, Some(SeatOccupant::Bot(_)));
            }
        }
        self.push_system_chat("Host updated table settings.");
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_take_seat(&mut self, user_id: &str, seat: SeatDto) {
        let idx = match seat {
            SeatDto::One => 0,
            SeatDto::Two => 1,
        };
        if self.seats.get(idx).is_some_and(Option::is_some) {
            self.send_error_to(user_id, ErrorCode::TableFull, "seat.take");
            return;
        }
        let is_invited = self.invited_ids.contains(user_id) || user_id == self.host_id;
        if self.settings.visibility == TableVisibilityDto::Protected && !is_invited {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "seat.take");
            return;
        }
        let Some(client) = self.watchers.remove(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "seat.take");
            return;
        };
        if client.user.guest {
            self.settings.rated = false;
        }
        if let Some(slot) = self.seats.get_mut(idx) {
            *slot = Some(SeatOccupant::Human(client));
        }
        if let Some(flag) = self.ready.get_mut(idx) {
            *flag = false;
        }
        self.emit_presence(user_id, UserStatusDto::Waiting, Some(self.id.clone()));
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_leave_seat(&mut self, user_id: &str) {
        let Some(idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "seat.leave");
            return;
        };
        if self.phase == TablePhaseDto::Playing
            && let Some(game) = self.active_game.as_mut()
            && game.resign(idx).is_ok()
            && let Some(finish) = game.finish_info()
        {
            let snap = game.snapshot_message(false);
            self.broadcast_table_msg(&snap);
            self.on_game_finished(finish);
        }
        let taken = self.seats.get_mut(idx).and_then(Option::take);
        if let Some(SeatOccupant::Human(client)) = taken {
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = false;
            }
            self.watchers.insert(user_id.to_owned(), client);
            self.emit_presence(user_id, UserStatusDto::Watching, Some(self.id.clone()));
            self.broadcast_update();
            self.emit_summary();
        }
    }

    fn handle_boot(&mut self, host_id: &str, target_id: &str) {
        if self.host_id != host_id {
            self.send_error_to(host_id, ErrorCode::NotHost, "boot");
            return;
        }
        if host_id == target_id {
            self.send_error_to(host_id, ErrorCode::Invalid, "boot");
            return;
        }

        let mut kicked_sink: Option<ClientSink> = None;
        if let Some(idx) = self.find_human_seat(target_id) {
            if let Some(SeatOccupant::Human(client)) =
                self.seats.get_mut(idx).and_then(Option::take)
            {
                kicked_sink = Some(client.sink);
            }
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = false;
            }
        } else if let Some(watcher) = self.watchers.remove(target_id) {
            kicked_sink = Some(watcher.sink);
        }

        let Some(sink) = kicked_sink else {
            self.send_error_to(host_id, ErrorCode::Invalid, "boot");
            return;
        };

        self.booted_until
            .insert(target_id.to_owned(), Instant::now() + BOOT_BLOCK_DURATION);
        self.hub.clear_user_table(target_id, &self.id);
        let _ = sink.send_msg(&ServerMessage::Kicked {
            reason: KickReasonDto::Booted,
        });
        self.emit_presence(target_id, UserStatusDto::Available, None);
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_add_bot(&mut self, host_id: &str, level: BotLevelDto) {
        if self.host_id != host_id {
            self.send_error_to(host_id, ErrorCode::NotHost, "bot.add");
            return;
        }
        if self.phase != TablePhaseDto::Waiting {
            self.send_error_to(host_id, ErrorCode::NotAllowed, "bot.add");
            return;
        }
        let Some(idx) = self.first_open_seat_index() else {
            self.send_error_to(host_id, ErrorCode::TableFull, "bot.add");
            return;
        };
        if let Some(slot) = self.seats.get_mut(idx) {
            *slot = Some(SeatOccupant::Bot(level));
        }
        if let Some(flag) = self.ready.get_mut(idx) {
            *flag = true;
        }
        self.settings.rated = false;
        self.push_system_chat("Computer opponent joined the table (unrated).");
        self.broadcast_update();
        self.emit_summary();
    }

    fn handle_remove_bot(&mut self, host_id: &str) {
        if self.host_id != host_id {
            self.send_error_to(host_id, ErrorCode::NotHost, "bot.remove");
            return;
        }
        if self.phase != TablePhaseDto::Waiting || self.is_house_bot {
            self.send_error_to(host_id, ErrorCode::NotAllowed, "bot.remove");
            return;
        }
        let mut removed = false;
        for idx in 0..2 {
            if self
                .seats
                .get(idx)
                .is_some_and(|s| matches!(s, Some(SeatOccupant::Bot(_))))
            {
                if let Some(slot) = self.seats.get_mut(idx) {
                    *slot = None;
                }
                if let Some(flag) = self.ready.get_mut(idx) {
                    *flag = false;
                }
                removed = true;
            }
        }
        if removed {
            self.broadcast_update();
            self.emit_summary();
        }
    }

    fn handle_update_member_user(&mut self, user: UserDto, muted: bool) {
        let mut changed = false;
        for occ in self.seats.iter_mut().flatten() {
            if let SeatOccupant::Human(client) = occ
                && client.user.id == user.id
            {
                client.user = user.clone();
                client.muted = muted;
                changed = true;
            }
        }
        if let Some(watcher) = self.watchers.get_mut(&user.id) {
            watcher.user = user;
            watcher.muted = muted;
            changed = true;
        }
        if changed {
            self.broadcast_update();
            self.emit_summary();
        }
    }

    fn promote_next_host(&mut self) {
        for occ in self.seats.iter().flatten() {
            if let SeatOccupant::Human(client) = occ {
                self.host_id = client.user.id.clone();
                self.push_system_chat(&format!("{} is now the table host.", client.user.name));
                return;
            }
        }
        if let Some(watcher) = self.watchers.values().next() {
            self.host_id = watcher.user.id.clone();
        } else {
            self.host_id.clear();
        }
    }

    fn shutdown_cleanup(&mut self) {
        if self.phase == TablePhaseDto::Playing {
            self.hub.dec_games();
        }
        for occ in self.seats.iter().flatten() {
            if let SeatOccupant::Human(c) = occ {
                self.hub.clear_user_table(&c.user.id, &self.id);
                let _ = c.sink.send_msg(&ServerMessage::Kicked {
                    reason: KickReasonDto::TableClosed,
                });
            }
        }
        for watcher in self.watchers.values() {
            self.hub.clear_user_table(&watcher.user.id, &self.id);
            let _ = watcher.sink.send_msg(&ServerMessage::Kicked {
                reason: KickReasonDto::TableClosed,
            });
        }
        self.hub.unregister_table(&self.id);
        let _ = self.lobby_events.try_send(LobbyEvent::TableClosed {
            table_id: self.id.clone(),
        });
    }

    fn summary(&self) -> TableSummaryDto {
        TableSummaryDto {
            id: self.id.clone(),
            no: self.no,
            game: self.game,
            seats: [
                self.seats[0].as_ref().map(|s| s.to_user_dto(self.phase)),
                self.seats[1].as_ref().map(|s| s.to_user_dto(self.phase)),
            ],
            visibility: self.settings.visibility,
            rated: self.settings.rated,
            timer: self.settings.timer,
            opts: self.settings.opts.clone(),
            watchers: u32::try_from(self.watchers.len()).unwrap_or(u32::MAX),
            phase: self.phase,
        }
    }

    fn snapshot(&self) -> TableSnapshotDto {
        let (turn, active_seat, ms_left) = self.active_game.as_ref().map_or_else(
            || (0, None, u64::from(self.settings.timer) * 1_000),
            |g| {
                let seat = if g.active_seat_index() == 0 {
                    Seat::One
                } else {
                    Seat::Two
                };
                (g.turn_number(), Some(seat_to_dto(seat)), g.ms_left())
            },
        );
        TableSnapshotDto {
            id: self.id.clone(),
            no: self.no,
            game: self.game,
            room: self.room_id.clone(),
            host: self.host_id.clone(),
            settings: self.settings.clone(),
            seats: [
                self.seats[0].as_ref().map(|s| s.to_user_dto(self.phase)),
                self.seats[1].as_ref().map(|s| s.to_user_dto(self.phase)),
            ],
            ready: self.ready,
            watchers: self.watcher_dtos(),
            phase: self.phase,
            chat: self.chat.iter().cloned().collect(),
            turn,
            active_seat,
            ms_left,
            ai_friend_available: self.hub.ai_friend().is_configured(),
            ai_friend: self.ai_chat.is_enabled(),
        }
    }

    fn watcher_dtos(&self) -> Vec<UserDto> {
        let mut list: Vec<UserDto> = self
            .watchers
            .values()
            .map(|c| {
                let mut dto = c.user.clone();
                dto.status = UserStatusDto::Watching;
                dto
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    fn broadcast_table_msg(&self, msg: &ServerMessage) {
        let sinks = self.all_sinks();
        broadcast_msg(sinks.iter(), msg);
    }

    fn broadcast_update(&self) {
        let msg = ServerMessage::TableUpdate(Box::new(TableUpdateDto {
            id: self.id.clone(),
            host: self.host_id.clone(),
            settings: self.settings.clone(),
            seats: [
                self.seats[0].as_ref().map(|s| s.to_user_dto(self.phase)),
                self.seats[1].as_ref().map(|s| s.to_user_dto(self.phase)),
            ],
            ready: self.ready,
            watchers: self.watcher_dtos(),
            phase: self.phase,
        }));
        self.broadcast_table_msg(&msg);
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

    fn prune_expired_chat(&mut self) {
        prune_expired_chat(&mut self.chat, Utc::now().timestamp_millis());
    }

    fn push_chat_and_broadcast(&mut self, line: ChatLineDto) {
        self.prune_expired_chat();
        if self.chat.len() >= MAX_CHAT_HISTORY {
            let _ = self.chat.pop_front();
        }
        self.chat.push_back(line.clone());
        let msg = ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        };
        self.broadcast_table_msg(&msg);
    }

    fn emit_summary(&self) {
        let _ = self
            .lobby_events
            .try_send(LobbyEvent::TableSummaryUpdated(self.summary()));
    }

    fn emit_presence(&self, user_id: &str, status: UserStatusDto, table_id: Option<String>) {
        let _ = self.lobby_events.try_send(LobbyEvent::UserPresenceChanged {
            user_id: user_id.to_owned(),
            status,
            table_id,
        });
    }

    fn all_sinks(&self) -> Vec<ClientSink> {
        let mut sinks = Vec::with_capacity(2 + self.watchers.len());
        for occ in self.seats.iter().flatten() {
            if let SeatOccupant::Human(c) = occ {
                sinks.push(c.sink.clone());
            }
        }
        for w in self.watchers.values() {
            sinks.push(w.sink.clone());
        }
        sinks
    }

    fn human_count(&self) -> usize {
        let seated_humans = self
            .seats
            .iter()
            .flatten()
            .filter(|occ| matches!(occ, SeatOccupant::Human(_)))
            .count();
        seated_humans + self.watchers.len()
    }

    fn first_open_seat_index(&self) -> Option<usize> {
        self.seats.iter().position(Option::is_none)
    }

    fn find_human_seat(&self, user_id: &str) -> Option<usize> {
        self.seats
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|occ| occ.wire_id() == user_id))
    }

    fn find_client_mut(&mut self, user_id: &str) -> Option<&mut TableClient> {
        for occ in self.seats.iter_mut().flatten() {
            if let SeatOccupant::Human(client) = occ
                && client.user.id == user_id
            {
                return Some(client);
            }
        }
        self.watchers.get_mut(user_id)
    }

    fn send_error_to(&self, user_id: &str, code: ErrorCode, re: &str) {
        let target_sink = self
            .seats
            .iter()
            .flatten()
            .find_map(|occ| match occ {
                SeatOccupant::Human(c) if c.user.id == user_id => Some(&c.sink),
                _ => None,
            })
            .or_else(|| self.watchers.get(user_id).map(|w| &w.sink));
        if let Some(sink) = target_sink {
            let _ = sink.send_msg(&ServerMessage::Error {
                code,
                re: re.to_owned(),
            });
        }
    }
}

//! Single-table actor (`tokio` task per table, zero shared locks).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use boomer_protocol::{
    BotLevelDto, ChatLineDto, ChatScopeDto, ErrorCode, GameKindDto, KickReasonDto, SeatDto,
    ServerMessage, TablePhaseDto, TableRoleDto, TableSettingsDto, TableSnapshotDto,
    TableSummaryDto, TableUpdateDto, TableVisibilityDto, UserDto, UserStatusDto,
};
use chrono::Utc;
use tokio::sync::{mpsc, oneshot};

use crate::actors::hub::LobbyHub;
use crate::actors::lobby_actor::LobbyEvent;
use crate::actors::{
    ClientSink, MAX_CHAT_HISTORY, bot_user_dto, broadcast_msg, sanitize_chat_text,
};

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
    /// Seat 1 occupant (always the initial host).
    pub host: TableClient,
    /// Optional Seat 2 human occupant (for `Play Now` or accepted invites).
    pub second_human: Option<TableClient>,
    /// Optional Seat 2 computer bot (for `play.now { vsBot }`).
    pub second_bot: Option<BotLevelDto>,
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
    watchers: HashMap<String, TableClient>,
    phase: TablePhaseDto,
    chat: VecDeque<ChatLineDto>,
    turn: u32,
    active_seat: Option<SeatDto>,
    booted_until: HashMap<String, Instant>,
    invited_ids: HashSet<String>,
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
        let host_id = init.host.user.id.clone();

        let mut settings = init.settings;
        if init.host.user.guest
            || init.second_bot.is_some()
            || init.second_human.as_ref().is_some_and(|c| c.user.guest)
        {
            settings.rated = false;
        }

        let second_seat = if let Some(human) = init.second_human {
            hub.set_user_table(&human.user.id, &init.id);
            Some(SeatOccupant::Human(human))
        } else {
            init.second_bot.map(SeatOccupant::Bot)
        };
        hub.set_user_table(&host_id, &init.id);

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
            seats: [Some(SeatOccupant::Human(init.host)), second_seat],
            ready: initial_ready,
            watchers: HashMap::new(),
            phase: TablePhaseDto::Waiting,
            chat: VecDeque::with_capacity(MAX_CHAT_HISTORY),
            turn: 0,
            active_seat: None,
            booted_until: HashMap::new(),
            invited_ids: HashSet::new(),
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
        while let Some(cmd) = self.rx.recv().await {
            let still_alive = self.handle_cmd(cmd);
            if !still_alive {
                break;
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
            TableCmd::Leave { user_id } => {
                self.handle_leave(&user_id);
            }
            TableCmd::SendChat { user_id, text } => {
                self.handle_chat(&user_id, &text);
            }
            TableCmd::ToggleReady { user_id } => {
                self.handle_ready(&user_id);
            }
            TableCmd::UpdateSettings { user_id, settings } => {
                self.handle_update_settings(&user_id, settings);
            }
            TableCmd::TakeSeat { user_id, seat } => {
                self.handle_take_seat(&user_id, seat);
            }
            TableCmd::LeaveSeat { user_id } => {
                self.handle_leave_seat(&user_id);
            }
            TableCmd::BootUser { host_id, target_id } => {
                self.handle_boot(&host_id, &target_id);
            }
            TableCmd::AddBot { host_id, level } => {
                self.handle_add_bot(&host_id, level);
            }
            TableCmd::RemoveBot { host_id } => {
                self.handle_remove_bot(&host_id);
            }
            TableCmd::AllowInvite { user_id } => {
                self.invited_ids.insert(user_id);
            }
            TableCmd::UpdateMemberUser { user, muted } => {
                self.handle_update_member_user(user, muted);
            }
        }
        self.human_count() > 0
    }

    fn handle_join(
        &mut self,
        client: TableClient,
        role: TableRoleDto,
    ) -> Result<TableSnapshotDto, ErrorCode> {
        let uid = client.user.id.clone();

        // Enforce 5-minute host boot block.
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

        // If already present at this table, return a fresh snapshot (reconnect / idempotent join).
        if self.find_human_seat(&uid).is_some() || self.watchers.contains_key(&uid) {
            return Ok(self.snapshot());
        }

        let new_status = match role {
            TableRoleDto::Player => {
                let open_idx = self.first_open_seat_index().ok_or(ErrorCode::TableFull)?;
                if client.user.guest {
                    self.settings.rated = false;
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
        let snap = self.snapshot();
        self.broadcast_update();
        self.emit_presence(&uid, new_status, Some(self.id.clone()));
        self.emit_summary();
        Ok(snap)
    }

    fn handle_leave(&mut self, user_id: &str) {
        let mut removed = false;
        if let Some(idx) = self.find_human_seat(user_id) {
            if let Some(slot) = self.seats.get_mut(idx) {
                *slot = None;
            }
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = false;
            }
            if self.phase == TablePhaseDto::Playing {
                self.phase = TablePhaseDto::Waiting;
                self.hub.dec_games();
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

        // Promote a remaining human to host if the host left.
        if self.host_id == user_id {
            self.promote_next_host();
        }

        if self.human_count() > 0 {
            self.broadcast_update();
            self.emit_summary();
        }
    }

    fn handle_chat(&mut self, user_id: &str, raw_text: &str) {
        let is_watcher = self.watchers.contains_key(user_id);
        let mute_spectators = self.settings.mute_spectators;
        let Some(client) = self.find_client_mut(user_id) else {
            return;
        };
        let sink = client.sink.clone();

        if client.muted {
            let _ = sink.send_msg(&ServerMessage::Error {
                code: ErrorCode::Muted,
                re: "chat.send".to_owned(),
            });
            return;
        }
        if is_watcher && mute_spectators {
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
        };
        self.push_chat_and_broadcast(line);
    }

    fn handle_ready(&mut self, user_id: &str) {
        let Some(idx) = self.find_human_seat(user_id) else {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "ready");
            return;
        };
        if self.phase != TablePhaseDto::Waiting {
            self.send_error_to(user_id, ErrorCode::NotAllowed, "ready");
            return;
        }
        if let Some(flag) = self.ready.get_mut(idx) {
            *flag = !*flag;
        }
        let both_occupied = self.seats[0].is_some() && self.seats[1].is_some();
        if both_occupied && self.ready == [true, true] {
            self.phase = TablePhaseDto::Playing;
            self.turn = 1;
            self.active_seat = Some(SeatDto::One);
            self.hub.inc_games();
            self.push_system_chat("Match ready! Both players confirmed.");
            for seat in self.seats.iter().flatten() {
                if let SeatOccupant::Human(c) = seat {
                    self.emit_presence(&c.user.id, UserStatusDto::Playing, Some(self.id.clone()));
                }
            }
        }
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
        let has_guest_or_bot = self.seats.iter().flatten().any(|occ| match occ {
            SeatOccupant::Bot(_) => true,
            SeatOccupant::Human(c) => c.user.guest,
        });
        if has_guest_or_bot {
            settings.rated = false;
        }
        self.settings = settings;
        // Require humans to re-confirm Ready after host changes settings.
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
        let taken = self.seats.get_mut(idx).and_then(Option::take);
        if let Some(SeatOccupant::Human(client)) = taken {
            if let Some(flag) = self.ready.get_mut(idx) {
                *flag = false;
            }
            if self.phase == TablePhaseDto::Playing {
                self.phase = TablePhaseDto::Waiting;
                self.hub.dec_games();
                self.push_system_chat(&format!("{} stood up and resigned.", client.user.name));
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
        if self.phase != TablePhaseDto::Waiting {
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
            turn: self.turn,
            active_seat: self.active_seat,
            ms_left: u64::from(self.settings.timer) * 1_000,
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
        let sinks = self.all_sinks();
        broadcast_msg(sinks.iter(), &msg);
    }

    fn push_system_chat(&mut self, text: &str) {
        self.push_chat_and_broadcast(ChatLineDto {
            from: None,
            name: None,
            text: text.to_owned(),
            ts: Utc::now().timestamp_millis(),
            sys: true,
        });
    }

    fn push_chat_and_broadcast(&mut self, line: ChatLineDto) {
        if self.chat.len() >= MAX_CHAT_HISTORY {
            let _ = self.chat.pop_front();
        }
        self.chat.push_back(line.clone());
        let msg = ServerMessage::Chat {
            scope: ChatScopeDto::Table,
            line,
        };
        let sinks = self.all_sinks();
        broadcast_msg(sinks.iter(), &msg);
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

//! Acyclic registry of live lounge rooms, active tables, online client sinks,
//! bounded bot AI worker semaphore, and match persistence handle.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use boomer_protocol::{GameKindDto, RoomSummaryDto, ServerMessage};
use tokio::sync::Semaphore;

use crate::actors::lobby_actor::{LobbyActor, LobbyHandle};
use crate::actors::table_actor::TableHandle;
use crate::actors::{ClientSink, broadcast_msg};
use crate::ports::MatchRepo;

/// Default soft capacity per lounge room (`incubation/plan.md` §5.3).
pub const DEFAULT_ROOM_CAPACITY: usize = 100;

/// Default occupancy threshold (70% of 100 users) triggering automatic room overflow.
pub const DEFAULT_AUTO_OPEN_THRESHOLD: u32 = 70;

/// Maximum concurrent blocking AI searches across all tables (`spawn_blocking` worker cap).
const BOT_WORKER_PERMITS: usize = 4;

struct RoomEntry {
    summary: RoomSummaryDto,
    handle: LobbyHandle,
}

/// Top-level coordinator tracking open rooms, table handles, and active user→table bindings.
///
/// Locks inside [`LobbyHub`] are standard synchronous [`RwLock`]s held ONLY for O(1) map
/// lookups/updates and NEVER across `.await`, preserving the strict acyclic actor hierarchy.
pub struct LobbyHub {
    rooms: RwLock<Vec<RoomEntry>>,
    tables: RwLock<HashMap<String, TableHandle>>,
    user_tables: RwLock<HashMap<String, String>>,
    connected_sinks: RwLock<HashMap<String, ClientSink>>,
    room_capacity: usize,
    auto_open_threshold: u32,
    next_table_no: AtomicU32,
    next_invite_id: AtomicU64,
    next_rng_seed: AtomicU64,
    online: Arc<AtomicU32>,
    tables_count: Arc<AtomicU32>,
    games_count: Arc<AtomicU32>,
    matches: Arc<dyn MatchRepo>,
    bot_semaphore: Arc<Semaphore>,
}

impl LobbyHub {
    /// Creates a new [`LobbyHub`] with initial Social Lounge rooms for 8-Ball Pool and Checkers.
    #[must_use]
    pub fn new(
        online: Arc<AtomicU32>,
        tables_count: Arc<AtomicU32>,
        games_count: Arc<AtomicU32>,
        matches: Arc<dyn MatchRepo>,
    ) -> Arc<Self> {
        Self::with_limits(
            online,
            tables_count,
            games_count,
            matches,
            DEFAULT_ROOM_CAPACITY,
            DEFAULT_AUTO_OPEN_THRESHOLD,
        )
    }

    /// Creates a [`LobbyHub`] with custom room capacity and auto-open threshold (used in tests).
    #[must_use]
    pub fn with_limits(
        online: Arc<AtomicU32>,
        tables_count: Arc<AtomicU32>,
        games_count: Arc<AtomicU32>,
        matches: Arc<dyn MatchRepo>,
        room_capacity: usize,
        auto_open_threshold: u32,
    ) -> Arc<Self> {
        let hub = Arc::new(Self {
            rooms: RwLock::new(Vec::new()),
            tables: RwLock::new(HashMap::new()),
            user_tables: RwLock::new(HashMap::new()),
            connected_sinks: RwLock::new(HashMap::new()),
            room_capacity,
            auto_open_threshold,
            next_table_no: AtomicU32::new(1),
            next_invite_id: AtomicU64::new(1),
            next_rng_seed: AtomicU64::new(0x00B0_0E42_0000_0001),
            online,
            tables_count,
            games_count,
            matches,
            bot_semaphore: Arc::new(Semaphore::new(BOT_WORKER_PERMITS)),
        });

        hub.open_next_room(GameKindDto::Pool);
        hub.open_next_room(GameKindDto::Checkers);
        hub
    }

    /// Returns the shared [`MatchRepo`] handle for atomic post-match persistence.
    #[must_use]
    pub fn matches_repo(&self) -> Arc<dyn MatchRepo> {
        Arc::clone(&self.matches)
    }

    /// Returns the bounded semaphore limiting concurrent blocking bot AI jobs.
    #[must_use]
    pub fn bot_semaphore(&self) -> Arc<Semaphore> {
        Arc::clone(&self.bot_semaphore)
    }

    /// Allocates a deterministic non-zero seed for racks and bot AI searches.
    #[must_use]
    pub fn alloc_seed(&self) -> u64 {
        self.next_rng_seed
            .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed)
    }

    /// Registers an active WebSocket connection's sink for global `rooms.update` broadcasts.
    pub fn register_connection(&self, sink: ClientSink) {
        if let Ok(mut guard) = self.connected_sinks.write() {
            guard.insert(sink.wire_id.clone(), sink);
        }
    }

    /// Unregisters a WebSocket connection sink on disconnect or identity change.
    pub fn unregister_connection(&self, wire_id: &str) {
        if let Ok(mut guard) = self.connected_sinks.write() {
            guard.remove(wire_id);
        }
    }

    /// Returns a snapshot of all currently open lounge rooms.
    #[must_use]
    pub fn list_rooms(&self) -> Vec<RoomSummaryDto> {
        self.rooms
            .read()
            .map(|guard| guard.iter().map(|r| r.summary.clone()).collect())
            .unwrap_or_default()
    }

    /// Looks up a room's [`LobbyHandle`] by `room_id`.
    #[must_use]
    pub fn find_room(&self, room_id: &str) -> Option<LobbyHandle> {
        self.rooms.read().ok().and_then(|guard| {
            guard
                .iter()
                .find(|r| r.summary.id == room_id)
                .map(|r| r.handle.clone())
        })
    }

    /// Updates cached occupancy for `room_id`.
    pub fn update_room_users(&self, room_id: &str, users: u32) {
        if let Ok(mut guard) = self.rooms.write()
            && let Some(entry) = guard.iter_mut().find(|r| r.summary.id == room_id)
        {
            entry.summary.users = users;
        }
    }

    /// Automatically opens an overflow room for `game` whenever all existing rooms for `game`
    /// have reached or exceeded `auto_open_threshold` (~70% capacity).
    pub fn ensure_room_capacity(self: &Arc<Self>, game: GameKindDto) {
        let needs_new = self.rooms.read().is_ok_and(|guard| {
            let mut matching = guard.iter().filter(|r| r.summary.game == game).peekable();
            matching.peek().is_some()
                && matching.all(|r| r.summary.users >= self.auto_open_threshold)
        });
        if needs_new {
            self.open_next_room(game);
            let rooms = self.list_rooms();
            if let Ok(guard) = self.connected_sinks.read() {
                broadcast_msg(guard.values(), &ServerMessage::RoomsUpdate { rooms });
            }
        }
    }

    fn open_next_room(self: &Arc<Self>, game: GameKindDto) {
        let index = self.rooms.read().map_or(1, |guard| {
            guard.iter().filter(|r| r.summary.game == game).count() + 1
        });

        let (prefix, label) = match game {
            GameKindDto::Pool => ("pool-social", "8-Ball Pool · Social Lounge"),
            GameKindDto::Checkers => ("checkers-social", "Checkers · Social Lounge"),
        };
        let id = format!("{prefix}-{index}");
        let name = if index == 1 {
            label.to_owned()
        } else {
            format!("{label} {index}")
        };

        let handle = LobbyActor::spawn(id.clone(), game, self.room_capacity, Arc::clone(self));
        let summary = RoomSummaryDto {
            id,
            game,
            name,
            tier: "social".to_owned(),
            users: 0,
        };
        if let Ok(mut guard) = self.rooms.write() {
            guard.push(RoomEntry { summary, handle });
        }
    }

    /// Allocates the next sequential table number (`1, 2, 3, ...`).
    #[must_use]
    pub fn alloc_table_no(&self) -> u32 {
        self.next_table_no.fetch_add(1, Ordering::Relaxed)
    }

    /// Allocates a unique invite identifier (`"inv1"`, `"inv2"`, ...).
    #[must_use]
    pub fn alloc_invite_id(&self) -> String {
        let n = self.next_invite_id.fetch_add(1, Ordering::Relaxed);
        format!("inv{n}")
    }

    /// Registers a live [`TableHandle`] and increments the global table gauge.
    pub fn register_table(&self, handle: TableHandle) {
        if let Ok(mut guard) = self.tables.write()
            && guard.insert(handle.table_id.clone(), handle).is_none()
        {
            self.tables_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Removes a closed table from the registry and decrements the global table gauge.
    pub fn unregister_table(&self, table_id: &str) {
        if let Ok(mut guard) = self.tables.write()
            && guard.remove(table_id).is_some()
        {
            self.tables_count.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Looks up a live [`TableHandle`] by `table_id` (supports shareable `/#table=<id>` links).
    #[must_use]
    pub fn find_table(&self, table_id: &str) -> Option<TableHandle> {
        self.tables
            .read()
            .ok()
            .and_then(|guard| guard.get(table_id).cloned())
    }

    /// Records `user_id` as currently present at `table_id`.
    pub fn set_user_table(&self, user_id: &str, table_id: &str) {
        if let Ok(mut guard) = self.user_tables.write() {
            guard.insert(user_id.to_owned(), table_id.to_owned());
        }
    }

    /// Clears `user_id`'s active table binding if it still matches `table_id`.
    pub fn clear_user_table(&self, user_id: &str, table_id: &str) {
        if let Ok(mut guard) = self.user_tables.write()
            && guard.get(user_id).is_some_and(|tid| tid == table_id)
        {
            guard.remove(user_id);
        }
    }

    /// Returns the active table ID for `user_id`, if any.
    #[must_use]
    pub fn user_table(&self, user_id: &str) -> Option<String> {
        self.user_tables
            .read()
            .ok()
            .and_then(|guard| guard.get(user_id).cloned())
    }

    /// Returns current server-wide connected user count.
    #[must_use]
    pub fn online_count(&self) -> u32 {
        self.online.load(Ordering::Relaxed)
    }

    /// Increments the live active games gauge.
    pub fn inc_games(&self) {
        self.games_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements the live active games gauge.
    pub fn dec_games(&self) {
        let mut cur = self.games_count.load(Ordering::Relaxed);
        while cur > 0 {
            match self.games_count.compare_exchange_weak(
                cur,
                cur - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }
}

import type {
  AimAidLevelDto,
  BallInHandRuleDto,
  BotLevelDto,
  CheckersClockModeDto,
  GameKindDto,
} from "../wasm_bridge.js";

export type UserStatusDto = "available" | "waiting" | "playing" | "watching";
export type TableVisibilityDto = "public" | "protected" | "private";
export type TablePhaseDto = "waiting" | "playing" | "over";
export type TableRoleDto = "player" | "spectator";
export type ChatScopeDto = "lobby" | "table";

export interface UserDto {
  readonly id: string;
  readonly name: string;
  readonly avatar: number;
  readonly rating: number;
  readonly prov: boolean;
  readonly status: UserStatusDto;
  readonly bot?: boolean;
  readonly guest?: boolean;
}

export interface RoomSummaryDto {
  readonly id: string;
  readonly game: GameKindDto;
  readonly name: string;
  readonly tier: string;
  readonly users: number;
}

export interface TableGameOptsDto {
  readonly aim?: AimAidLevelDto;
  readonly callEight?: boolean;
  readonly bih?: BallInHandRuleDto;
  readonly clock?: CheckersClockModeDto;
  readonly gameMin?: number;
}

export interface TableSettingsDto {
  readonly type: TableVisibilityDto;
  readonly rated: boolean;
  readonly timer: number;
  readonly muteSpectators: boolean;
  readonly opts: TableGameOptsDto;
}

export interface TableSummaryDto {
  readonly id: string;
  readonly no: number;
  readonly game: GameKindDto;
  readonly seats: readonly [UserDto | null, UserDto | null];
  readonly type: TableVisibilityDto;
  readonly rated: boolean;
  readonly timer: number;
  readonly opts: TableGameOptsDto;
  readonly watchers: number;
  readonly phase: TablePhaseDto;
}

export interface ChatLineDto {
  readonly from?: string;
  readonly name?: string;
  readonly text: string;
  readonly ts: number;
  readonly sys?: boolean;
}

export interface LobbySnapshotDto {
  readonly room: string;
  readonly game: GameKindDto;
  readonly me: UserDto;
  readonly users: readonly UserDto[];
  readonly tables: readonly TableSummaryDto[];
  readonly chat: readonly ChatLineDto[];
  readonly online: number;
  readonly ignored?: readonly string[];
}

export interface TableSnapshotDto {
  readonly id: string;
  readonly no: number;
  readonly game: GameKindDto;
  readonly room: string;
  readonly host: string;
  readonly settings: TableSettingsDto;
  readonly seats: readonly [UserDto | null, UserDto | null];
  readonly ready: readonly [boolean, boolean];
  readonly watchers: readonly UserDto[];
  readonly phase: TablePhaseDto;
  readonly chat: readonly ChatLineDto[];
  readonly turn: number;
  readonly activeSeat?: "1" | "2" | null;
  readonly msLeft: number;
}

export interface TableUpdateDto {
  readonly id: string;
  readonly host: string;
  readonly settings: TableSettingsDto;
  readonly seats: readonly [UserDto | null, UserDto | null];
  readonly ready: readonly [boolean, boolean];
  readonly watchers: readonly UserDto[];
  readonly phase: TablePhaseDto;
}

export interface PendingInviteView {
  readonly inviteId: string;
  readonly from: UserDto;
  readonly game: GameKindDto;
  readonly tableId?: string;
  readonly settings: TableSettingsDto;
  readonly msLeft: number;
}

export interface HighscoreRowDto {
  readonly rank: number;
  readonly id: string;
  readonly name: string;
  readonly avatar: number;
  readonly wins: number;
  readonly games: number;
}

export type ConnectionStatus = "idle" | "probing" | "offline_host" | "connecting" | "connected";

export interface OnlineState {
  status: ConnectionStatus;
  me: UserDto | null;
  rooms: RoomSummaryDto[];
  activeRoom: string | null;
  activeGame: GameKindDto | null;
  onlineCount: number;
  users: UserDto[];
  tables: TableSummaryDto[];
  lobbyChat: ChatLineDto[];
  activeTable: TableSnapshotDto | null;
  ignored: Set<string>;
  pendingInvites: PendingInviteView[];
  playNowState: "idle" | "queued" | "suggest_bot";
  highscores: { game: GameKindDto; rows: HighscoreRowDto[] } | null;
  authNotice: string | null;
  isNewAccount: boolean;
  lastError: string | null;
}

const SESSION_KEY = "boomer_session_token_v1";
const GUEST_KEY = "boomer_guest_token_v1";
const PROTOCOL_VERSION = 1;

function readStorage(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStorage(key: string, value: string | null): void {
  try {
    if (value === null) {
      window.localStorage.removeItem(key);
    } else {
      window.localStorage.setItem(key, value);
    }
  } catch {
    // Ignore restricted storage environments.
  }
}

export function parseHashIntent(): { readonly loginToken?: string; readonly tableId?: string } {
  const hash = window.location.hash;
  if (hash.startsWith("#login=")) {
    const token = decodeURIComponent(hash.slice("#login=".length)).trim();
    if (token !== "") {
      return { loginToken: token };
    }
  }
  if (hash.startsWith("#table=")) {
    const tableId = decodeURIComponent(hash.slice("#table=".length)).trim();
    if (tableId !== "") {
      return { tableId };
    }
  }
  return {};
}

export class OnlineSession {
  readonly state: OnlineState = {
    status: "idle",
    me: null,
    rooms: [],
    activeRoom: null,
    activeGame: null,
    onlineCount: 0,
    users: [],
    tables: [],
    lobbyChat: [],
    activeTable: null,
    ignored: new Set<string>(),
    pendingInvites: [],
    playNowState: "idle",
    highscores: null,
    authNotice: null,
    isNewAccount: false,
    lastError: null,
  };

  private ws: WebSocket | null = null;
  private backoffMs = 1_000;
  private reconnectTimer: number | null = null;
  private intentionallyClosed = false;
  private pendingLoginToken: string | null = null;
  private pendingTableLink: string | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly gameFrameListeners = new Set<(rawJson: string) => void>();
  public latestGameFrameJson: string | null = null;

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  subscribeGameFrames(fn: (rawJson: string) => void): () => void {
    this.gameFrameListeners.add(fn);
    return () => {
      this.gameFrameListeners.delete(fn);
    };
  }

  private emitGameFrame(rawJson: string): void {
    for (const fn of this.gameFrameListeners) {
      fn(rawJson);
    }
  }

  private notify(): void {
    for (const fn of this.listeners) {
      fn();
    }
  }

  async probeBackend(): Promise<boolean> {
    if (this.state.status === "connected" || this.state.status === "connecting") {
      return true;
    }
    this.state.status = "probing";
    this.notify();
    try {
      const resp = await fetch("/status", {
        method: "GET",
        headers: { Accept: "application/json" },
        cache: "no-store",
      });
      if (!resp.ok) {
        this.state.status = "offline_host";
        this.notify();
        return false;
      }
      const data = (await resp.json()) as { status?: unknown };
      if (typeof data.status === "string") {
        this.state.status = "idle";
        this.notify();
        return true;
      }
    } catch {
      // No backend reachable (e.g., GitHub Pages static deploy).
    }
    this.state.status = "offline_host";
    this.notify();
    return false;
  }

  connect(intent?: { readonly loginToken?: string; readonly tableId?: string }): void {
    if (intent?.loginToken !== undefined) {
      this.pendingLoginToken = intent.loginToken;
      window.history.replaceState(null, "", window.location.pathname + window.location.search);
    }
    if (intent?.tableId !== undefined) {
      this.pendingTableLink = intent.tableId;
    }
    if (this.ws !== null && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      this.flushIntents();
      return;
    }
    this.intentionallyClosed = false;
    this.openSocket();
  }

  disconnect(): void {
    this.intentionallyClosed = true;
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws !== null) {
      this.ws.close();
      this.ws = null;
    }
    this.state.status = "idle";
    this.latestGameFrameJson = null;
    this.notify();
  }

  send(type: string, data: Record<string, unknown>): void {
    if (this.ws === null || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }
    this.ws.send(JSON.stringify({ t: type, d: data }));
  }

  sendRawJson(rawJson: string): void {
    if (this.ws === null || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }
    this.ws.send(rawJson);
  }

  joinRoom(roomId: string): void {
    this.state.playNowState = "idle";
    this.state.activeTable = null;
    this.latestGameFrameJson = null;
    this.send("lobby.join", { room: roomId });
    this.notify();
  }

  sendChat(text: string, scope: ChatScopeDto): void {
    const trimmed = text.trim();
    if (trimmed === "") {
      return;
    }
    this.send("chat.send", { text: trimmed, scope });
  }

  createTable(settings: TableSettingsDto): void {
    this.send("table.create", settings as unknown as Record<string, unknown>);
  }

  joinTable(tableId: string, role: TableRoleDto): void {
    this.send("table.join", { tableId, as: role });
  }

  leaveTable(): void {
    this.send("table.leave", {});
    this.state.activeTable = null;
    this.state.playNowState = "idle";
    this.latestGameFrameJson = null;
    this.notify();
  }

  playNow(vsBot?: BotLevelDto): void {
    this.state.playNowState = vsBot === undefined ? "queued" : "idle";
    this.send("play.now", vsBot === undefined ? {} : { vsBot });
    this.notify();
  }

  sendInvite(to: string, tableId?: string): void {
    const payload: Record<string, unknown> = { to };
    if (tableId !== undefined) {
      payload.tableId = tableId;
    }
    this.send("invite.send", payload);
  }

  answerInvite(inviteId: string, accept: boolean): void {
    this.state.pendingInvites = this.state.pendingInvites.filter((i) => i.inviteId !== inviteId);
    this.send("invite.answer", { inviteId, accept });
    this.notify();
  }

  toggleIgnore(userId: string): void {
    const next = !this.state.ignored.has(userId);
    if (next) {
      this.state.ignored.add(userId);
    } else {
      this.state.ignored.delete(userId);
    }
    this.send("ignore.set", { userId, on: next });
    this.notify();
  }

  toggleReady(): void {
    this.send("ready", {});
  }

  updateTableSettings(settings: TableSettingsDto): void {
    this.send("settings.set", settings as unknown as Record<string, unknown>);
  }

  takeSeat(seat: "1" | "2"): void {
    this.send("seat.take", { seat });
  }

  standUp(): void {
    this.send("seat.leave", {});
  }

  bootUser(userId: string): void {
    this.send("boot", { userId });
  }

  addBot(level: BotLevelDto): void {
    this.send("bot.add", { level });
  }

  removeBot(): void {
    this.send("bot.remove", {});
  }

  requestMagicLink(email: string): void {
    this.state.authNotice = null;
    this.send("auth.email", { email: email.trim() });
    this.notify();
  }

  updateAccount(name?: string, avatar?: number): void {
    const payload: Record<string, unknown> = {};
    if (name !== undefined && name.trim() !== "") {
      payload.name = name.trim();
    }
    if (avatar !== undefined) {
      payload.avatar = avatar;
    }
    this.send("account.update", payload);
  }

  logout(): void {
    writeStorage(SESSION_KEY, null);
    this.state.isNewAccount = false;
    this.send("auth.logout", {});
  }

  deleteAccount(): void {
    writeStorage(SESSION_KEY, null);
    this.state.isNewAccount = false;
    this.send("account.delete", {});
  }

  fetchHighscores(game: GameKindDto, around = false): void {
    this.send("highscores.get", { game, around });
  }

  clearError(): void {
    this.state.lastError = null;
    this.notify();
  }

  private openSocket(): void {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${proto}//${window.location.host}/ws`;
    this.state.status = "connecting";
    this.notify();

    const ws = new WebSocket(wsUrl);
    this.ws = ws;

    ws.addEventListener("open", () => {
      this.backoffMs = 1_000;
      const session = readStorage(SESSION_KEY);
      const guest = readStorage(GUEST_KEY);
      const hello: Record<string, unknown> = { v: PROTOCOL_VERSION };
      if (session !== null) {
        hello.session = session;
      }
      if (guest !== null) {
        hello.guest = guest;
      }
      ws.send(JSON.stringify({ t: "hello", d: hello }));
    });

    ws.addEventListener("message", (ev) => {
      if (typeof ev.data !== "string") {
        return;
      }
      try {
        const env = JSON.parse(ev.data) as { t: string; d: Record<string, unknown> };
        this.handleServerMessage(env.t, env.d, ev.data);
      } catch {
        // Ignore malformed server frames.
      }
    });

    ws.addEventListener("close", () => {
      this.ws = null;
      if (this.intentionallyClosed) {
        this.state.status = "idle";
        this.notify();
        return;
      }
      this.state.status = "connecting";
      this.notify();
      const delay = this.backoffMs;
      this.backoffMs = Math.min(15_000, this.backoffMs * 2);
      this.reconnectTimer = window.setTimeout(() => {
        this.reconnectTimer = null;
        if (!this.intentionallyClosed) {
          this.openSocket();
        }
      }, delay);
    });
  }

  private flushIntents(): void {
    if (this.pendingLoginToken !== null) {
      const tok = this.pendingLoginToken;
      this.pendingLoginToken = null;
      this.send("auth.magic", { token: tok });
    }
    if (this.pendingTableLink !== null) {
      const tid = this.pendingTableLink;
      this.pendingTableLink = null;
      this.send("table.join", { tableId: tid, as: "player" });
    }
  }

  private handleServerMessage(type: string, d: Record<string, unknown>, rawJson: string): void {
    switch (type) {
      case "welcome": {
        this.state.status = "connected";
        this.state.me = (d.me as UserDto | undefined) ?? null;
        if (typeof d.guest === "string") {
          writeStorage(GUEST_KEY, d.guest);
        }
        const rooms = (d.rooms as RoomSummaryDto[] | undefined) ?? [];
        this.state.rooms = rooms;
        this.flushIntents();
        const targetRoom = this.state.activeRoom ?? rooms[0]?.id ?? null;
        if (targetRoom !== null && this.state.activeTable === null) {
          this.send("lobby.join", { room: targetRoom });
        }
        break;
      }
      case "rooms.update":
        this.state.rooms = (d.rooms as RoomSummaryDto[] | undefined) ?? [];
        break;
      case "lobby.snapshot": {
        const snap = d as unknown as LobbySnapshotDto;
        this.state.activeRoom = snap.room;
        this.state.activeGame = snap.game;
        this.state.me = snap.me;
        this.state.users = [...snap.users];
        this.state.tables = [...snap.tables];
        this.state.lobbyChat = [...snap.chat];
        this.state.onlineCount = snap.online;
        this.state.ignored = new Set(snap.ignored ?? []);
        break;
      }
      case "user.add": {
        const u = d as unknown as UserDto;
        this.state.users = [...this.state.users.filter((x) => x.id !== u.id), u].sort((a, b) =>
          a.name.localeCompare(b.name),
        );
        break;
      }
      case "user.update": {
        const u = d as unknown as UserDto;
        this.state.users = this.state.users.map((x) => (x.id === u.id ? u : x));
        if (this.state.me?.id === u.id) {
          this.state.me = u;
        }
        break;
      }
      case "user.remove": {
        const id = String(d.id ?? "");
        this.state.users = this.state.users.filter((x) => x.id !== id);
        break;
      }
      case "table.upsert": {
        const tbl = d as unknown as TableSummaryDto;
        const idx = this.state.tables.findIndex((t) => t.id === tbl.id);
        if (idx >= 0) {
          const copy = [...this.state.tables];
          copy[idx] = tbl;
          this.state.tables = copy;
        } else {
          this.state.tables = [...this.state.tables, tbl].sort((a, b) => a.no - b.no);
        }
        break;
      }
      case "table.remove": {
        const id = String(d.id ?? "");
        this.state.tables = this.state.tables.filter((t) => t.id !== id);
        break;
      }
      case "chat": {
        const scope = d.scope as ChatScopeDto;
        const line: ChatLineDto = {
          from: typeof d.from === "string" ? d.from : undefined,
          name: typeof d.name === "string" ? d.name : undefined,
          text: String(d.text ?? ""),
          ts: Number(d.ts ?? Date.now()),
          sys: Boolean(d.sys),
        };
        if (scope === "table" && this.state.activeTable !== null) {
          const nextChat = [...this.state.activeTable.chat, line].slice(-50);
          this.state.activeTable = { ...this.state.activeTable, chat: nextChat };
        } else {
          this.state.lobbyChat = [...this.state.lobbyChat, line].slice(-50);
        }
        break;
      }
      case "table.snapshot": {
        const nextTable = d as unknown as TableSnapshotDto;
        if (this.state.activeTable?.id !== nextTable.id) {
          this.latestGameFrameJson = null;
        }
        this.state.activeTable = nextTable;
        this.state.playNowState = "idle";
        break;
      }
      case "table.update": {
        const upd = d as unknown as TableUpdateDto;
        if (this.state.activeTable !== null && this.state.activeTable.id === upd.id) {
          if (upd.phase === "waiting") {
            this.latestGameFrameJson = null;
          }
          this.state.activeTable = {
            ...this.state.activeTable,
            host: upd.host,
            settings: upd.settings,
            seats: upd.seats,
            ready: upd.ready,
            watchers: upd.watchers,
            phase: upd.phase,
          };
        }
        break;
      }
      case "checkers.snapshot":
      case "pool.snapshot": {
        this.latestGameFrameJson = rawJson;
        if (this.state.activeTable !== null && this.state.activeTable.phase === "waiting") {
          this.state.activeTable = { ...this.state.activeTable, phase: "playing" };
        }
        this.notify();
        this.emitGameFrame(rawJson);
        return;
      }
      case "pool.shot": {
        if (d.state !== undefined) {
          this.latestGameFrameJson = JSON.stringify({ t: "pool.snapshot", d: { state: d.state } });
        }
        if (this.state.activeTable !== null && this.state.activeTable.phase === "waiting") {
          this.state.activeTable = { ...this.state.activeTable, phase: "playing" };
          this.notify();
        }
        this.emitGameFrame(rawJson);
        return;
      }
      case "kicked": {
        this.state.activeTable = null;
        this.latestGameFrameJson = null;
        this.state.lastError =
          d.reason === "booted" ? "booted_from_table" : "table_closed";
        break;
      }
      case "game.over": {
        if (this.state.activeTable !== null) {
          this.state.activeTable = {
            ...this.state.activeTable,
            phase: "over",
          };
        }
        break;
      }
      case "rematch.state": {
        if (this.state.activeTable !== null && Array.isArray(d.ready)) {
          const r = d.ready as [boolean, boolean];
          this.state.activeTable = {
            ...this.state.activeTable,
            ready: [Boolean(r[0]), Boolean(r[1])],
          };
        }
        break;
      }
      case "game.pause": {
        this.state.authNotice = "opponent_disconnected_paused";
        break;
      }
      case "game.resume": {
        this.state.authNotice = null;
        break;
      }
      case "invite.received": {
        const inv = d as unknown as PendingInviteView;
        this.state.pendingInvites = [
          ...this.state.pendingInvites.filter((i) => i.inviteId !== inv.inviteId),
          inv,
        ];
        break;
      }
      case "invite.closed": {
        const inviteId = String(d.inviteId ?? "");
        this.state.pendingInvites = this.state.pendingInvites.filter(
          (i) => i.inviteId !== inviteId,
        );
        break;
      }
      case "play.now.queued":
        this.state.playNowState = "queued";
        break;
      case "play.now.timeout":
        this.state.playNowState = "suggest_bot";
        break;
      case "auth.sent":
        this.state.authNotice = "magic_link_sent";
        break;
      case "auth.ok": {
        if (typeof d.session === "string") {
          writeStorage(SESSION_KEY, d.session);
        }
        this.state.me = (d.me as UserDto | undefined) ?? this.state.me;
        this.state.isNewAccount = Boolean(d.isNew);
        this.state.authNotice = "signed_in";
        break;
      }
      case "highscores": {
        this.state.highscores = {
          game: d.game as GameKindDto,
          rows: (d.rows as HighscoreRowDto[] | undefined) ?? [],
        };
        break;
      }
      case "error": {
        const code = String(d.code ?? "invalid");
        if (code === "upgrade_required") {
          window.location.reload();
          return;
        }
        this.state.lastError = code;
        break;
      }
      default:
        break;
    }
    this.notify();
  }
}

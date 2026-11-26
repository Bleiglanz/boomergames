import type { Strings } from "../i18n/strings.js";
import type {
  ChatLineDto,
  OnlineSession,
  TableSettingsDto,
  TableSnapshotDto,
  TableSummaryDto,
  TableVisibilityDto,
  UserDto,
} from "../online/session.js";
import type { BotLevelDto, GameKindDto } from "../wasm_bridge.js";

const AVATAR_BADGES: readonly string[] = [
  "🎱",
  "♟️",
  "🎯",
  "🏆",
  "🦊",
  "🦉",
  "🐻",
  "🐧",
  "🎩",
  "⚓",
  "⭐",
  "🍀",
  "🎸",
  "☕",
  "🧭",
  "👑",
];

type LoungeTab = "tables" | "players" | "chat";
type TableTab = "table" | "chat" | "host";
type ActiveModal = "none" | "create_table" | "account" | "highscores" | "profile";

interface GoogleCredentialResponse {
  readonly credential?: string;
}

interface GoogleAccountsIdApi {
  initialize(config: {
    readonly client_id: string;
    readonly callback: (resp: GoogleCredentialResponse) => void;
    readonly auto_select?: boolean;
  }): void;
  renderButton(
    parent: HTMLElement,
    options: {
      readonly theme?: "outline" | "filled_blue" | "filled_black";
      readonly size?: "large" | "medium" | "small";
      readonly text?: "signin_with" | "signup_with" | "continue_with";
      readonly width?: number;
    },
  ): void;
}

interface GoogleGlobal {
  readonly accounts?: {
    readonly id?: GoogleAccountsIdApi;
  };
}

let gsiLoadPromise: Promise<GoogleAccountsIdApi | null> | null = null;

function loadGoogleIdentity(): Promise<GoogleAccountsIdApi | null> {
  const win = window as unknown as { google?: GoogleGlobal };
  if (win.google?.accounts?.id !== undefined) {
    return Promise.resolve(win.google.accounts.id);
  }
  if (gsiLoadPromise !== null) {
    return gsiLoadPromise;
  }
  gsiLoadPromise = new Promise<GoogleAccountsIdApi | null>((resolve) => {
    const existing = document.querySelector<HTMLScriptElement>(
      'script[src="https://accounts.google.com/gsi/client"]',
    );
    const finish = (): void => {
      resolve(win.google?.accounts?.id ?? null);
    };
    if (existing !== null) {
      existing.addEventListener("load", finish, { once: true });
      existing.addEventListener("error", () => resolve(null), { once: true });
      return;
    }
    const script = document.createElement("script");
    script.src = "https://accounts.google.com/gsi/client";
    script.async = true;
    script.defer = true;
    script.addEventListener("load", finish, { once: true });
    script.addEventListener("error", () => resolve(null), { once: true });
    document.head.append(script);
  });
  return gsiLoadPromise;
}

function avatarGlyph(index: number): string {
  return AVATAR_BADGES[index % AVATAR_BADGES.length] ?? "🎱";
}

function formatTime(ts: number): string {
  const date = new Date(ts);
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

function makeBtn(
  label: string,
  onClick: () => void,
  opts?: { readonly primary?: boolean; readonly active?: boolean; readonly disabled?: boolean; readonly small?: boolean },
): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  const classes = ["classic-btn"];
  if (opts?.primary) {
    classes.push("primary");
  }
  if (opts?.active) {
    classes.push("active-toggle");
  }
  if (opts?.small) {
    classes.push("btn-sm");
  }
  btn.className = classes.join(" ");
  btn.textContent = label;
  btn.disabled = opts?.disabled ?? false;
  btn.addEventListener("click", onClick);
  return btn;
}

export class LobbyController {
  private readonly root: HTMLElement;
  private readonly session: OnlineSession;
  private strings: Strings;
  private readonly onReturnOffline: () => void;
  private loungeTab: LoungeTab = "tables";
  private tableTab: TableTab = "table";
  private modal: ActiveModal = "none";
  private highscoresGame: GameKindDto = "pool";
  private copiedNotice = false;
  private unsubscribe: (() => void) | null = null;

  constructor(
    root: HTMLElement,
    session: OnlineSession,
    strings: Strings,
    onReturnOffline: () => void,
  ) {
    this.root = root;
    this.session = session;
    this.strings = strings;
    this.onReturnOffline = onReturnOffline;
    this.unsubscribe = session.subscribe(() => this.render());
    this.render();
  }

  setStrings(strings: Strings): void {
    this.strings = strings;
    this.render();
  }

  dispose(): void {
    if (this.unsubscribe !== null) {
      this.unsubscribe();
      this.unsubscribe = null;
    }
  }

  render(): void {
    const s = this.strings;
    const st = this.session.state;
    this.root.innerHTML = "";

    const shell = document.createElement("div");
    shell.className = "lounge-shell";

    // Offline server fallback notice (e.g., static GitHub Pages deploy).
    if (st.status === "offline_host") {
      const card = document.createElement("section");
      card.className = "game-card lounge-offline-card";
      const h2 = document.createElement("h2");
      h2.textContent = s.loungeOfflineTitle;
      const p = document.createElement("p");
      p.textContent = s.loungeOfflineDesc;
      const row = document.createElement("div");
      row.className = "control-row";
      row.append(
        makeBtn(s.loungePlayOfflineBtn, this.onReturnOffline, { primary: true }),
        makeBtn(s.loungeRetryBtn, () => {
          void this.session.probeBackend().then((ok) => {
            if (ok) {
              this.session.connect();
            }
          });
        }),
      );
      card.append(h2, p, row);
      shell.append(card);
      this.root.append(shell);
      return;
    }

    if (st.status === "probing" || st.status === "connecting" || st.status === "idle") {
      const banner = document.createElement("div");
      banner.className = "notice-banner";
      banner.textContent = s.loungeConnecting;
      shell.append(banner);
      this.root.append(shell);
      return;
    }

    // Error toast
    if (st.lastError !== null) {
      const errBanner = document.createElement("div");
      errBanner.className = "notice-banner danger-banner";
      errBanner.setAttribute("role", "alert");
      const text = document.createElement("span");
      text.textContent = `${s.statusLabel}: ${st.lastError}`;
      errBanner.append(text, makeBtn("✕", () => this.session.clearError(), { small: true }));
      shell.append(errBanner);
    }

    // Incoming 30s invite toasts
    for (const inv of st.pendingInvites) {
      const toast = document.createElement("div");
      toast.className = "notice-banner invite-toast";
      toast.setAttribute("role", "status");
      const msg = document.createElement("span");
      msg.textContent = `${s.loungeInviteFrom} ${inv.from.name} (${inv.game === "pool" ? s.poolTitle : s.checkersTitle})`;
      const actions = document.createElement("div");
      actions.className = "control-row";
      actions.append(
        makeBtn(s.loungeAccept, () => this.session.answerInvite(inv.inviteId, true), {
          primary: true,
          small: true,
        }),
        makeBtn(s.loungeDecline, () => this.session.answerInvite(inv.inviteId, false), {
          small: true,
        }),
      );
      toast.append(msg, actions);
      shell.append(toast);
    }

    // Play Now waiting / bot suggestion banner
    if (st.playNowState === "queued") {
      const qBanner = document.createElement("div");
      qBanner.className = "notice-banner";
      qBanner.textContent = s.loungePlayNowWaiting;
      shell.append(qBanner);
    } else if (st.playNowState === "suggest_bot") {
      const bBanner = document.createElement("div");
      bBanner.className = "notice-banner";
      const txt = document.createElement("span");
      txt.textContent = s.loungePlayNowTimeout;
      const row = document.createElement("div");
      row.className = "control-row";
      row.append(
        makeBtn(s.botEasy, () => this.session.playNow("easy"), { small: true }),
        makeBtn(s.botMedium, () => this.session.playNow("medium"), { primary: true, small: true }),
        makeBtn(s.botHard, () => this.session.playNow("hard"), { small: true }),
      );
      bBanner.append(txt, row);
      shell.append(bBanner);
    }

    // Top Lounge Action Bar (rooms, quick actions, account)
    shell.append(this.buildLoungeHeader());

    // Either active Table Room or 3-panel Lounge
    if (st.activeTable !== null) {
      shell.append(this.buildTableRoomView(st.activeTable));
    } else {
      shell.append(this.buildLoungeWorkspace());
    }

    // Active modal overlay
    if (this.modal === "create_table") {
      shell.append(this.buildCreateTableModal());
    } else if (this.modal === "account" || st.needsNicknameReview) {
      shell.append(this.buildAccountModal());
    } else if (this.modal === "highscores") {
      shell.append(this.buildHighscoresModal());
    } else if (this.modal === "profile") {
      shell.append(this.buildProfileModal());
    }

    this.root.append(shell);
  }

  private buildLoungeHeader(): HTMLElement {
    const s = this.strings;
    const st = this.session.state;

    const bar = document.createElement("section");
    bar.className = "lounge-toolbar";

    const roomGroup = document.createElement("div");
    roomGroup.className = "lounge-room-picker";
    const roomLabel = document.createElement("label");
    roomLabel.htmlFor = "boomer-room-select";
    roomLabel.textContent = s.loungeRoomLabel;
    const select = document.createElement("select");
    select.id = "boomer-room-select";
    select.className = "classic-select";
    for (const room of st.rooms) {
      const opt = document.createElement("option");
      opt.value = room.id;
      opt.textContent = `${room.name} (${room.users})`;
      opt.selected = room.id === st.activeRoom;
      select.append(opt);
    }
    select.addEventListener("change", () => {
      this.session.joinRoom(select.value);
    });
    roomGroup.append(roomLabel, select);

    const actions = document.createElement("div");
    actions.className = "lounge-actions";
    if (st.activeTable === null) {
      actions.append(
        makeBtn(s.loungePlayNow, () => this.session.playNow(), { primary: true }),
        makeBtn(s.loungeCreateTable, () => {
          this.modal = "create_table";
          this.render();
        }),
      );
    }
    actions.append(
      makeBtn(s.loungeHighscores, () => {
        this.highscoresGame = st.activeGame ?? "pool";
        this.modal = "highscores";
        this.session.fetchHighscores(this.highscoresGame, false);
        this.render();
      }),
    );

    const meLabel =
      st.me !== null
        ? `${avatarGlyph(st.me.avatar)} ${st.me.name}${st.me.guest ? "" : ` (${st.me.rating}${st.me.prov ? "?" : ""})`}`
        : s.loungeSignIn;
    actions.append(
      makeBtn(meLabel, () => {
        this.modal = "account";
        this.render();
      }),
    );

    bar.append(roomGroup, actions);
    return bar;
  }

  private buildLoungeWorkspace(): HTMLElement {
    const s = this.strings;
    const st = this.session.state;
    const wrap = document.createElement("div");
    wrap.className = "lounge-workspace";

    // Mobile/tablet segmented tabs (< 960px)
    const tabs = document.createElement("div");
    tabs.className = "mobile-tabs";
    tabs.setAttribute("role", "tablist");
    const tabItems: readonly [LoungeTab, string][] = [
      ["tables", `${s.loungeTablesTab} (${st.tables.length})`],
      ["players", `${s.loungePlayersTab} (${st.users.length})`],
      ["chat", s.loungeChatTab],
    ];
    for (const [id, label] of tabItems) {
      const btn = makeBtn(
        label,
        () => {
          this.loungeTab = id;
          this.render();
        },
        { active: this.loungeTab === id },
      );
      btn.setAttribute("role", "tab");
      btn.setAttribute("aria-selected", String(this.loungeTab === id));
      tabs.append(btn);
    }
    wrap.append(tabs);

    const grid = document.createElement("div");
    grid.className = "lounge-grid";

    // Left column: Tables + Room Chat
    const left = document.createElement("div");
    left.className = "lounge-main-col";

    const tablesPanel = this.buildTablesPanel(st.tables);
    tablesPanel.dataset.mobileActive = String(this.loungeTab === "tables");

    const chatPanel = this.buildChatPanel(st.lobbyChat, "lobby", s.loungeRoomChatTitle);
    chatPanel.dataset.mobileActive = String(this.loungeTab === "chat");

    left.append(tablesPanel, chatPanel);

    // Right column: Live User Roster
    const rosterPanel = this.buildRosterPanel(st.users);
    rosterPanel.dataset.mobileActive = String(this.loungeTab === "players");

    grid.append(left, rosterPanel);
    wrap.append(grid);
    return wrap;
  }

  private buildTablesPanel(tables: readonly TableSummaryDto[]): HTMLElement {
    const s = this.strings;
    const panel = document.createElement("section");
    panel.className = "lounge-panel lounge-tables-panel";

    const header = document.createElement("div");
    header.className = "panel-heading-row";
    const h2 = document.createElement("h2");
    h2.textContent = `${s.loungeTablesTab} (${tables.length})`;
    header.append(h2);
    panel.append(header);

    if (tables.length === 0) {
      const empty = document.createElement("p");
      empty.className = "lounge-empty";
      empty.textContent = s.loungeNoTables;
      panel.append(empty);
      return panel;
    }

    const list = document.createElement("div");
    list.className = "table-cards";
    for (const t of tables) {
      const card = document.createElement("div");
      card.className = "table-row-card";

      const meta = document.createElement("div");
      meta.className = "table-row-meta";
      const title = document.createElement("div");
      title.className = "table-row-title";
      const s1 = t.seats[0] !== null ? `${t.seats[0].bot ? "🤖 " : ""}${t.seats[0].name}` : s.loungeOpenSeat;
      const s2 = t.seats[1] !== null ? `${t.seats[1].bot ? "🤖 " : ""}${t.seats[1].name}` : s.loungeOpenSeat;
      title.textContent = `#${t.no} · ${s1} vs ${s2}`;

      const badges = document.createElement("div");
      badges.className = "table-row-badges";
      badges.textContent = [
        t.type,
        t.rated ? s.loungeRated : s.loungeUnrated,
        t.timer > 0 ? `${t.timer}s` : s.clockUnlimited,
        `👁 ${t.watchers}`,
        t.phase,
      ].join(" · ");
      meta.append(title, badges);

      const btns = document.createElement("div");
      btns.className = "table-row-actions";
      const full = t.seats[0] !== null && t.seats[1] !== null;
      if (!full && t.type === "public") {
        btns.append(
          makeBtn(s.loungeJoin, () => this.session.joinTable(t.id, "player"), {
            primary: true,
            small: true,
          }),
        );
      } else if (full) {
        btns.append(makeBtn(s.loungeFull, () => undefined, { disabled: true, small: true }));
      }
      if (t.type !== "private") {
        btns.append(
          makeBtn(s.loungeWatch, () => this.session.joinTable(t.id, "spectator"), { small: true }),
        );
      }

      card.append(meta, btns);
      list.append(card);
    }

    panel.append(list);
    return panel;
  }

  private buildRosterPanel(users: readonly UserDto[]): HTMLElement {
    const s = this.strings;
    const st = this.session.state;
    const panel = document.createElement("aside");
    panel.className = "lounge-panel lounge-roster-panel";

    const h2 = document.createElement("h2");
    h2.textContent = `${s.loungePlayersTab} (${users.length})`;
    panel.append(h2);

    const list = document.createElement("div");
    list.className = "roster-list";
    for (const u of users) {
      const row = document.createElement("div");
      row.className = "roster-item";

      const info = document.createElement("div");
      info.className = "roster-info";
      const canViewProfile = !u.guest && !u.bot && u.id.startsWith("u");
      if (canViewProfile) {
        const nameBtn = document.createElement("button");
        nameBtn.type = "button";
        nameBtn.className = "roster-name profile-link-btn";
        nameBtn.textContent = `${avatarGlyph(u.avatar)} ${u.name}`;
        nameBtn.addEventListener("click", () => this.openProfile(u.id));
        info.append(nameBtn);
      } else {
        const name = document.createElement("div");
        name.className = "roster-name";
        name.textContent = `${avatarGlyph(u.avatar)} ${u.name}`;
        info.append(name);
      }
      const sub = document.createElement("div");
      sub.className = "roster-sub";
      const ratingText = u.guest ? "Guest" : `${u.rating}${u.prov ? "?" : ""}`;
      sub.textContent = `${ratingText} · ${u.status}`;
      info.append(sub);

      row.append(info);

      if (st.me !== null && u.id !== st.me.id) {
        const actions = document.createElement("div");
        actions.className = "roster-actions";
        if (u.status === "available") {
          actions.append(
            makeBtn(s.loungeInviteBtn, () => this.session.sendInvite(u.id), { small: true }),
          );
        }
        const isIgnored = st.ignored.has(u.id);
        actions.append(
          makeBtn(
            isIgnored ? s.loungeUnignore : s.loungeIgnore,
            () => this.session.toggleIgnore(u.id),
            { active: isIgnored, small: true },
          ),
        );
        row.append(actions);
      }

      list.append(row);
    }

    panel.append(list);
    return panel;
  }

  private buildChatPanel(
    lines: readonly ChatLineDto[],
    scope: "lobby" | "table",
    title: string,
  ): HTMLElement {
    const s = this.strings;
    const ignored = this.session.state.ignored;

    const panel = document.createElement("section");
    panel.className = "lounge-panel lounge-chat-panel";

    const h3 = document.createElement("h3");
    h3.textContent = title;
    panel.append(h3);

    const log = document.createElement("div");
    log.className = "chat-log";
    log.setAttribute("role", "log");
    log.setAttribute("aria-live", "polite");

    const visible = lines.filter((line) => line.from === undefined || !ignored.has(line.from));
    if (visible.length === 0) {
      const empty = document.createElement("div");
      empty.className = "chat-sys-line";
      empty.textContent = s.loungeChatEmpty;
      log.append(empty);
    } else {
      for (const line of visible) {
        const item = document.createElement("div");
        item.className = line.sys ? "chat-sys-line" : "chat-line";
        const ts = document.createElement("span");
        ts.className = "chat-ts";
        ts.textContent = `[${formatTime(line.ts)}] `;
        item.append(ts);
        if (!line.sys && line.name !== undefined) {
          const author = document.createElement("strong");
          author.textContent = `${line.name}: `;
          item.append(author);
        }
        item.append(document.createTextNode(line.text));
        log.append(item);
      }
      queueMicrotask(() => {
        log.scrollTop = log.scrollHeight;
      });
    }

    const form = document.createElement("form");
    form.className = "chat-form";
    const input = document.createElement("input");
    input.type = "text";
    input.className = "classic-input";
    input.maxLength = 300;
    input.placeholder = s.loungeChatPlaceholder;
    input.setAttribute("aria-label", s.loungeChatPlaceholder);

    const sendBtn = document.createElement("button");
    sendBtn.type = "submit";
    sendBtn.className = "classic-btn primary";
    sendBtn.textContent = s.loungeSendBtn;

    form.addEventListener("submit", (ev) => {
      ev.preventDefault();
      if (input.value.trim() !== "") {
        this.session.sendChat(input.value, scope);
        input.value = "";
      }
    });

    form.append(input, sendBtn);
    panel.append(log, form);
    return panel;
  }

  private buildTableRoomView(table: TableSnapshotDto): HTMLElement {
    const s = this.strings;
    const me = this.session.state.me;
    const isHost = me !== null && table.host === me.id;
    const mySeatIdx =
      me === null
        ? -1
        : table.seats[0]?.id === me.id
          ? 0
          : table.seats[1]?.id === me.id
            ? 1
            : -1;

    const wrap = document.createElement("div");
    wrap.className = "lounge-workspace";

    // Mobile/tablet segmented tabs for Table Room
    const tabs = document.createElement("div");
    tabs.className = "mobile-tabs";
    tabs.setAttribute("role", "tablist");
    const tabItems: readonly [TableTab, string][] = [
      ["table", `${s.loungeTableTab} #${table.no}`],
      ["chat", s.loungeChatTab],
      ["host", s.loungeControlsTab],
    ];
    for (const [id, label] of tabItems) {
      tabs.append(
        makeBtn(
          label,
          () => {
            this.tableTab = id;
            this.render();
          },
          { active: this.tableTab === id },
        ),
      );
    }
    wrap.append(tabs);

    const grid = document.createElement("div");
    grid.className = "lounge-grid";

    // Left column: Table seats + status + Table Chat
    const left = document.createElement("div");
    left.className = "lounge-main-col";

    const tablePanel = document.createElement("section");
    tablePanel.className = "lounge-panel";
    tablePanel.dataset.mobileActive = String(this.tableTab === "table");

    const titleRow = document.createElement("div");
    titleRow.className = "panel-heading-row";
    const h2 = document.createElement("h2");
    h2.textContent = `${table.game === "pool" ? s.poolTitle : s.checkersTitle} · #${table.no} (${table.phase})`;
    const topBtns = document.createElement("div");
    topBtns.className = "control-row";
    topBtns.append(
      makeBtn(
        this.copiedNotice ? s.loungeLinkCopied : s.loungeCopyLink,
        () => {
          const link = `${window.location.origin}${window.location.pathname}#table=${table.id}`;
          void navigator.clipboard?.writeText(link);
          this.copiedNotice = true;
          this.render();
          window.setTimeout(() => {
            this.copiedNotice = false;
            this.render();
          }, 2_000);
        },
        { small: true },
      ),
      makeBtn(s.loungeLeaveTable, () => this.session.leaveTable(), { small: true }),
    );
    titleRow.append(h2, topBtns);

    // Seats strip
    const seatsStrip = document.createElement("div");
    seatsStrip.className = "players-strip";
    ([0, 1] as const).forEach((idx) => {
      const occ = table.seats[idx];
      const ready = table.ready[idx];
      const card = document.createElement("div");
      card.className = ready ? "player-card active" : "player-card";

      const info = document.createElement("div");
      const nameEl = document.createElement("div");
      nameEl.className = "player-name";
      if (occ !== null) {
        const hostTag = occ.id === table.host ? " ★" : "";
        nameEl.textContent = `${occ.bot ? "🤖" : avatarGlyph(occ.avatar)} ${occ.name}${hostTag}`;
      } else {
        nameEl.textContent = `${s.loungeSeat} ${idx + 1}: ${s.loungeOpenSeat}`;
      }
      const sub = document.createElement("div");
      sub.className = "player-sub";
      sub.textContent =
        occ !== null
          ? ready
            ? s.loungeReadyBadge
            : s.loungeNotReadyBadge
          : s.loungeWaitingPlayer;
      info.append(nameEl, sub);
      card.append(info);

      if (occ === null && mySeatIdx === -1) {
        const seatDto = idx === 0 ? "1" : "2";
        card.append(
          makeBtn(s.loungeSitDown, () => this.session.takeSeat(seatDto), {
            primary: true,
            small: true,
          }),
        );
      } else if (isHost && occ !== null && !occ.bot && occ.id !== me.id) {
        card.append(
          makeBtn(s.loungeBoot, () => this.session.bootUser(occ.id), { small: true }),
        );
      }

      seatsStrip.append(card);
    });

    // Primary Ready / Stand Up row
    const actionRow = document.createElement("div");
    actionRow.className = "control-row";
    if (mySeatIdx !== -1 && table.phase === "waiting") {
      const myReady = table.ready[mySeatIdx] ?? false;
      actionRow.append(
        makeBtn(
          myReady ? s.loungeUnreadyBtn : s.loungeReadyBtn,
          () => this.session.toggleReady(),
          { primary: !myReady, active: myReady },
        ),
      );
    }
    if (mySeatIdx !== -1) {
      actionRow.append(makeBtn(s.loungeStandUp, () => this.session.standUp()));
    }

    const metaNote = document.createElement("div");
    metaNote.className = "status-box";
    metaNote.textContent = `${table.settings.type} · ${table.settings.rated ? s.loungeRated : s.loungeUnrated} · ${table.settings.timer > 0 ? `${table.settings.timer}s` : s.clockUnlimited} · ${table.settings.muteSpectators ? s.loungeSpectatorsMuted : s.loungeSpectatorsOpen}`;

    tablePanel.append(titleRow, seatsStrip, actionRow, metaNote);

    const chatPanel = this.buildChatPanel(table.chat, "table", s.loungeTableChatTitle);
    chatPanel.dataset.mobileActive = String(this.tableTab === "chat");

    left.append(tablePanel, chatPanel);

    // Right column: Host Controls & Spectators
    const right = document.createElement("aside");
    right.className = "lounge-panel lounge-roster-panel";
    right.dataset.mobileActive = String(this.tableTab === "host");

    const h3 = document.createElement("h3");
    h3.textContent = s.loungeControlsTab;
    right.append(h3);

    if (isHost && table.phase === "waiting") {
      const hostBox = document.createElement("div");
      hostBox.className = "control-group";

      const muteToggle = makeBtn(
        table.settings.muteSpectators ? s.loungeUnmuteSpectators : s.loungeMuteSpectators,
        () =>
          this.session.updateTableSettings({
            ...table.settings,
            muteSpectators: !table.settings.muteSpectators,
          }),
        { active: table.settings.muteSpectators, small: true },
      );
      hostBox.append(muteToggle);

      const hasBot = table.seats.some((seat) => seat?.bot === true);
      const botRow = document.createElement("div");
      botRow.className = "control-row";
      if (hasBot) {
        botRow.append(
          makeBtn(s.loungeRemoveBot, () => this.session.removeBot(), { small: true }),
        );
      } else if (table.seats[0] === null || table.seats[1] === null) {
        const levels: readonly [BotLevelDto, string][] = [
          ["easy", s.botEasy],
          ["medium", s.botMedium],
          ["hard", s.botHard],
        ];
        for (const [lvl, label] of levels) {
          botRow.append(makeBtn(label, () => this.session.addBot(lvl), { small: true }));
        }
      }
      hostBox.append(botRow);
      right.append(hostBox);
    }

    const watchHeading = document.createElement("h3");
    watchHeading.textContent = `${s.loungeSpectators} (${table.watchers.length})`;
    right.append(watchHeading);

    const wList = document.createElement("div");
    wList.className = "roster-list";
    if (table.watchers.length === 0) {
      const none = document.createElement("p");
      none.className = "lounge-empty";
      none.textContent = s.loungeNoSpectators;
      wList.append(none);
    } else {
      for (const w of table.watchers) {
        const row = document.createElement("div");
        row.className = "roster-item";
        const name = document.createElement("span");
        name.textContent = `${avatarGlyph(w.avatar)} ${w.name}`;
        row.append(name);
        if (isHost && w.id !== me.id) {
          row.append(
            makeBtn(s.loungeBoot, () => this.session.bootUser(w.id), { small: true }),
          );
        }
        wList.append(row);
      }
    }
    right.append(wList);

    grid.append(left, right);
    wrap.append(grid);
    return wrap;
  }

  private buildCreateTableModal(): HTMLElement {
    const s = this.strings;
    const overlay = document.createElement("div");
    overlay.className = "modal-backdrop";

    const dialog = document.createElement("div");
    dialog.className = "modal-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");

    const h2 = document.createElement("h2");
    h2.textContent = s.loungeCreateTable;

    const visSelect = document.createElement("select");
    visSelect.className = "classic-select";
    const visOpts: readonly [TableVisibilityDto, string][] = [
      ["public", s.loungeVisPublic],
      ["protected", s.loungeVisProtected],
      ["private", s.loungeVisPrivate],
    ];
    for (const [val, label] of visOpts) {
      const opt = document.createElement("option");
      opt.value = val;
      opt.textContent = label;
      visSelect.append(opt);
    }

    const timerSelect = document.createElement("select");
    timerSelect.className = "classic-select";
    for (const sec of [30, 45, 60, 0]) {
      const opt = document.createElement("option");
      opt.value = String(sec);
      opt.textContent = sec === 0 ? s.clockUnlimited : `${sec}s`;
      timerSelect.append(opt);
    }

    const ratedBox = document.createElement("input");
    ratedBox.type = "checkbox";
    ratedBox.checked = !(this.session.state.me?.guest ?? true);
    ratedBox.disabled = this.session.state.me?.guest ?? true;
    const ratedLabel = document.createElement("label");
    ratedLabel.className = "check-row";
    ratedLabel.append(ratedBox, document.createTextNode(` ${s.loungeRated}`));

    const muteBox = document.createElement("input");
    muteBox.type = "checkbox";
    const muteLabel = document.createElement("label");
    muteLabel.className = "check-row";
    muteLabel.append(muteBox, document.createTextNode(` ${s.loungeMuteSpectators}`));

    const btnRow = document.createElement("div");
    btnRow.className = "control-row";
    btnRow.append(
      makeBtn(
        s.loungeCreateConfirm,
        () => {
          const settings: TableSettingsDto = {
            type: visSelect.value as TableVisibilityDto,
            rated: ratedBox.checked,
            timer: Number(timerSelect.value),
            muteSpectators: muteBox.checked,
            opts: {},
          };
          this.modal = "none";
          this.session.createTable(settings);
        },
        { primary: true },
      ),
      makeBtn(s.loungeCancel, () => {
        this.modal = "none";
        this.render();
      }),
    );

    dialog.append(h2, visSelect, timerSelect, ratedLabel, muteLabel, btnRow);
    overlay.append(dialog);
    return overlay;
  }

  private buildAccountModal(): HTMLElement {
    const s = this.strings;
    const st = this.session.state;
    const me = st.me;

    const overlay = document.createElement("div");
    overlay.className = "modal-backdrop";

    const dialog = document.createElement("div");
    dialog.className = "modal-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");

    const h2 = document.createElement("h2");
    h2.textContent = me !== null && !me.guest ? s.loungeAccountTitle : s.loungeSignIn;
    dialog.append(h2);

    if (st.authNotice !== null) {
      const note = document.createElement("div");
      note.className = "notice-banner";
      note.textContent =
        st.authNotice === "magic_link_sent" ? s.loungeMagicSent : s.loungeSignedIn;
      dialog.append(note);
    }

    if (me === null || me.guest) {
      if (st.googleClientId !== null) {
        const clientId = st.googleClientId;
        const googleSlot = document.createElement("div");
        googleSlot.className = "google-signin-slot";
        googleSlot.textContent = s.loungeGoogleLoading;
        dialog.append(googleSlot);

        void loadGoogleIdentity().then((gis) => {
          if (!googleSlot.isConnected) {
            return;
          }
          if (gis === null) {
            googleSlot.remove();
            return;
          }
          googleSlot.textContent = "";
          gis.initialize({
            client_id: clientId,
            callback: (resp) => {
              if (typeof resp.credential === "string" && resp.credential !== "") {
                this.session.signInWithGoogle(resp.credential);
              }
            },
          });
          gis.renderButton(googleSlot, {
            theme: "outline",
            size: "large",
            text: "signin_with",
            width: 280,
          });
        });

        const divider = document.createElement("div");
        divider.className = "auth-divider";
        divider.textContent = s.loungeOrDivider;
        dialog.append(divider);
      }

      const p = document.createElement("p");
      p.textContent = s.loungeSignInDesc;

      const form = document.createElement("form");
      form.className = "control-group";
      const emailInput = document.createElement("input");
      emailInput.type = "email";
      emailInput.required = true;
      emailInput.className = "classic-input";
      emailInput.placeholder = "you@example.com";

      const submitBtn = document.createElement("button");
      submitBtn.type = "submit";
      submitBtn.className = "classic-btn primary";
      submitBtn.textContent = s.loungeSendMagicLink;

      const row = document.createElement("div");
      row.className = "control-row";
      row.append(
        submitBtn,
        makeBtn(s.loungeCancel, () => {
          this.modal = "none";
          this.render();
        }),
      );
      form.addEventListener("submit", (ev) => {
        ev.preventDefault();
        if (emailInput.value.trim() !== "") {
          this.session.requestMagicLink(emailInput.value);
        }
      });
      form.append(emailInput, row);
      dialog.append(p, form);
    } else {
      const form = document.createElement("form");
      form.className = "control-group";

      const nameLabel = document.createElement("label");
      nameLabel.htmlFor = "boomer-nickname-input";
      nameLabel.textContent = s.loungeNicknameLabel;

      const nameInput = document.createElement("input");
      nameInput.id = "boomer-nickname-input";
      nameInput.type = "text";
      nameInput.className = "classic-input";
      nameInput.minLength = 3;
      nameInput.maxLength = 20;
      nameInput.required = true;
      nameInput.value = me.name;

      const hint = document.createElement("p");
      hint.className = "keys-hint";
      hint.textContent = s.loungeNicknameHint;

      let selectedAvatar = me.avatar;
      const avatarGrid = document.createElement("div");
      avatarGrid.className = "avatar-grid";
      AVATAR_BADGES.forEach((glyph, idx) => {
        const b = makeBtn(
          glyph,
          () => {
            selectedAvatar = idx;
            for (const child of Array.from(avatarGrid.children)) {
              child.classList.remove("active-toggle");
            }
            b.classList.add("active-toggle");
          },
          { active: idx === selectedAvatar, small: true },
        );
        avatarGrid.append(b);
      });

      const saveBtn = document.createElement("button");
      saveBtn.type = "submit";
      saveBtn.className = "classic-btn primary";
      saveBtn.textContent = s.loungeSaveAccount;

      const row = document.createElement("div");
      row.className = "control-row";
      row.append(
        saveBtn,
        makeBtn(s.loungeProfileView, () => this.openProfile(me.id)),
        makeBtn(s.loungeLogout, () => {
          this.modal = "none";
          this.session.logout();
        }),
        makeBtn(s.loungeDeleteAccount, () => {
          if (window.confirm(s.loungeDeleteConfirm)) {
            this.modal = "none";
            this.session.deleteAccount();
          }
        }),
        makeBtn(s.loungeCancel, () => {
          this.modal = "none";
          this.session.dismissNicknameReview();
        }),
      );

      form.addEventListener("submit", (ev) => {
        ev.preventDefault();
        this.modal = "none";
        this.session.updateAccount(nameInput.value, selectedAvatar);
      });

      form.append(nameLabel, nameInput, hint, avatarGrid, row);
      dialog.append(form);
    }

    overlay.append(dialog);
    return overlay;
  }

  private buildHighscoresModal(): HTMLElement {
    const s = this.strings;
    const hs = this.session.state.highscores;

    const overlay = document.createElement("div");
    overlay.className = "modal-backdrop";

    const dialog = document.createElement("div");
    dialog.className = "modal-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");

    const h2 = document.createElement("h2");
    h2.textContent = s.loungeHighscores;

    const switchRow = document.createElement("div");
    switchRow.className = "control-row";
    switchRow.append(
      makeBtn(
        s.poolTitle,
        () => {
          this.highscoresGame = "pool";
          this.session.fetchHighscores("pool", false);
        },
        { active: this.highscoresGame === "pool", small: true },
      ),
      makeBtn(
        s.checkersTitle,
        () => {
          this.highscoresGame = "checkers";
          this.session.fetchHighscores("checkers", false);
        },
        { active: this.highscoresGame === "checkers", small: true },
      ),
      makeBtn(
        s.loungeFindMe,
        () => this.session.fetchHighscores(this.highscoresGame, true),
        { small: true },
      ),
    );

    const list = document.createElement("div");
    list.className = "move-list";
    if (hs === null || hs.rows.length === 0) {
      list.textContent = s.loungeNoHighscores;
    } else {
      for (const row of hs.rows) {
        const line = document.createElement("div");
        line.className = "move-row profile-hs-row";
        const nameBtn = document.createElement("button");
        nameBtn.type = "button";
        nameBtn.className = "profile-link-btn";
        nameBtn.textContent = `#${row.rank} ${avatarGlyph(row.avatar)} ${row.name}`;
        nameBtn.addEventListener("click", () => this.openProfile(row.id));
        const stats = document.createElement("span");
        stats.textContent = `${row.wins}W / ${row.games}G`;
        line.append(nameBtn, stats);
        list.append(line);
      }
    }

    dialog.append(
      h2,
      switchRow,
      list,
      makeBtn(s.loungeCancel, () => {
        this.modal = "none";
        this.render();
      }),
    );
    overlay.append(dialog);
    return overlay;
  }

  private openProfile(userId: string): void {
    this.modal = "profile";
    this.session.fetchProfile(userId);
  }

  private buildProfileModal(): HTMLElement {
    const s = this.strings;
    const profile = this.session.state.activeProfile;

    const overlay = document.createElement("div");
    overlay.className = "modal-backdrop";

    const dialog = document.createElement("div");
    dialog.className = "modal-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");

    const h2 = document.createElement("h2");
    if (profile === null) {
      h2.textContent = s.loungeProfileTitle;
      const loading = document.createElement("p");
      loading.className = "lounge-empty";
      loading.textContent = s.loungeProfileLoading;
      dialog.append(h2, loading);
    } else {
      h2.textContent = `${avatarGlyph(profile.user.avatar)} ${profile.user.name}`;
      const since = document.createElement("div");
      since.className = "roster-sub";
      const sinceDate = new Date(profile.since).toLocaleDateString();
      since.textContent = `${s.loungeProfileSince}: ${sinceDate}`;
      dialog.append(h2, since);

      const statsHeading = document.createElement("h3");
      statsHeading.textContent = s.loungeProfileStats;
      const statsGrid = document.createElement("div");
      statsGrid.className = "profile-stats-grid";
      const gameOrder: readonly GameKindDto[] = ["pool", "checkers"];
      for (const gk of gameOrder) {
        const st = profile.stats.find((x) => x.game === gk);
        const card = document.createElement("div");
        card.className = "table-row-card profile-stat-card";
        const title = document.createElement("div");
        title.className = "table-row-title";
        title.textContent = gk === "pool" ? s.poolTitle : s.checkersTitle;
        const val = document.createElement("div");
        val.className = "table-row-badges";
        if (st !== undefined) {
          val.textContent = `Elo ${st.rating}${st.games < 10 ? "?" : ""} · ${st.wins}W / ${st.losses}L / ${st.draws}D (${st.games}G)`;
        } else {
          val.textContent = "Elo 1200? · 0W / 0L / 0D";
        }
        card.append(title, val);
        statsGrid.append(card);
      }
      dialog.append(statsHeading, statsGrid);

      const recentHeading = document.createElement("h3");
      recentHeading.textContent = s.loungeProfileRecent;
      const list = document.createElement("div");
      list.className = "move-list";
      if (profile.last.length === 0) {
        list.textContent = s.loungeProfileNoMatches;
      } else {
        for (const m of profile.last) {
          const row = document.createElement("div");
          row.className = "move-row profile-match-row";
          const gameLabel = m.game === "pool" ? "🎱" : "♟️";
          const resBadge = m.result === "win" ? "W" : m.result === "loss" ? "L" : "D";
          const dateStr = new Date(m.ts).toLocaleDateString();
          row.textContent = `${gameLabel} [${resBadge}] vs ${m.opp} · ${dateStr}`;
          list.append(row);
        }
      }
      dialog.append(recentHeading, list);
    }

    dialog.append(
      makeBtn(s.loungeCancel, () => {
        this.modal = "none";
        this.render();
      }),
    );
    overlay.append(dialog);
    return overlay;
  }
}

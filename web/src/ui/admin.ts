/**
 * Strictly Google-authenticated Admin Console served at `/antonadmin`.
 * Zero cookies: bearer token lives in `sessionStorage` (or reuses an existing
 * Google-authenticated `boomer_session_token_v1` in `localStorage`).
 */

interface GoogleCredentialResponse {
  readonly credential?: string;
}

interface GoogleAccountsIdApi {
  initialize(config: {
    readonly client_id: string;
    readonly callback: (response: GoogleCredentialResponse) => void;
  }): void;
  renderButton(
    parent: HTMLElement,
    options: {
      readonly theme?: "outline" | "filled_blue" | "filled_black";
      readonly size?: "large" | "medium" | "small";
      readonly width?: number;
    },
  ): void;
}

interface GoogleGlobalWindow extends Window {
  google?: { readonly accounts?: { readonly id?: GoogleAccountsIdApi } };
}

interface AdminUserStat {
  readonly game: string;
  readonly rating: number;
  readonly ratedGames: number;
  readonly games: number;
  readonly wins: number;
}

interface AdminUser {
  readonly id: number;
  readonly wireId: string;
  readonly email: string;
  readonly displayName: string;
  readonly googleLinked: boolean;
  readonly isModerator: boolean;
  readonly mutedUntil: string | null;
  readonly bannedUntil: string | null;
  readonly createdAt: string;
  readonly lastLoginAt: string | null;
  readonly lastLoginProvider: string | null;
  readonly online: boolean;
  readonly stats: readonly AdminUserStat[];
}

interface AdminActivity {
  readonly id: number;
  readonly eventType: string;
  readonly actorUserId: number | null;
  readonly actorLabel: string;
  readonly authProvider: string | null;
  readonly game: string | null;
  readonly details: string | null;
  readonly createdAt: string;
}

interface AdminMatch {
  readonly id: number;
  readonly game: string;
  readonly player1Label: string;
  readonly player2Label: string;
  readonly botLevel: number | null;
  readonly winner: number | null;
  readonly endReason: string;
  readonly rated: boolean;
  readonly endedAt: string;
}

interface AdminReport {
  readonly id: number;
  readonly reporterName: string | null;
  readonly reportedId: number | null;
  readonly reportedName: string;
  readonly reason: string;
  readonly chatLog: string | null;
  readonly createdAt: string;
  readonly resolvedAt: string | null;
}

interface AdminOverview {
  readonly adminEmail: string;
  readonly server: {
    readonly version: string;
    readonly uptimeS: number;
    readonly dbOk: boolean;
    readonly maintenance: boolean;
    readonly maintenanceMessage: string;
    readonly online: number;
    readonly tables: number;
    readonly games: number;
    readonly draining: boolean;
    readonly totalUsers: number;
    readonly totalMatches: number;
    readonly openReports: number;
  };
  readonly rooms: readonly {
    readonly id: string;
    readonly game: string;
    readonly name: string;
    readonly users: number;
  }[];
  readonly activeTables: readonly {
    readonly tableId: string;
    readonly roomId: string;
  }[];
  readonly onlineSessions: readonly {
    readonly wireId: string;
    readonly kind: string;
    readonly userId: number | null;
    readonly displayName: string;
    readonly email: string | null;
    readonly activeTable: string | null;
  }[];
  readonly users: readonly AdminUser[];
  readonly activity: readonly AdminActivity[];
  readonly recentMatches: readonly AdminMatch[];
  readonly reports: readonly AdminReport[];
  readonly aiFriend: {
    readonly configured: boolean;
    readonly model: string;
    readonly callsUsed: number;
    readonly maxCalls: number;
    readonly promptFile: string | null;
    readonly activePrompt: string;
    readonly usingCustomFile: boolean;
  };
}

type AdminTab = "overview" | "users" | "activity" | "matches" | "ai";
type ActivityFilter = "all" | "logins" | "anonymous" | "offline" | "admin";

const ADMIN_TOKEN_KEY = "boomer_admin_token_v1";
const SESSION_TOKEN_KEY = "boomer_session_token_v1";
const ADMIN_EMAIL = "antonrechenauer@gmail.com";

let gsiScriptPromise: Promise<GoogleAccountsIdApi | null> | null = null;

function loadGoogleIdentityScript(): Promise<GoogleAccountsIdApi | null> {
  const existing = (window as GoogleGlobalWindow).google?.accounts?.id;
  if (existing !== undefined) {
    return Promise.resolve(existing);
  }
  if (gsiScriptPromise !== null) {
    return gsiScriptPromise;
  }
  gsiScriptPromise = new Promise((resolve) => {
    const script = document.createElement("script");
    script.src = "https://accounts.google.com/gsi/client";
    script.async = true;
    script.defer = true;
    script.onload = () => {
      resolve((window as GoogleGlobalWindow).google?.accounts?.id ?? null);
    };
    script.onerror = () => {
      gsiScriptPromise = null;
      resolve(null);
    };
    document.head.append(script);
  });
  return gsiScriptPromise;
}

function readAdminToken(): string | null {
  try {
    const adminTok = window.sessionStorage.getItem(ADMIN_TOKEN_KEY);
    if (adminTok !== null && adminTok.trim() !== "") {
      return adminTok;
    }
    const sessTok = window.localStorage.getItem(SESSION_TOKEN_KEY);
    if (sessTok !== null && sessTok.trim() !== "") {
      return sessTok;
    }
  } catch {
    // Ignore restricted storage.
  }
  return null;
}

function writeAdminToken(token: string | null): void {
  try {
    if (token === null) {
      window.sessionStorage.removeItem(ADMIN_TOKEN_KEY);
    } else {
      window.sessionStorage.setItem(ADMIN_TOKEN_KEY, token);
    }
  } catch {
    // Ignore restricted storage.
  }
}

function formatDateTime(iso: string | null): string {
  if (iso === null || iso === "") {
    return "—";
  }
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) {
    return iso;
  }
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  const rem = secs % 60;
  if (days > 0) {
    return `${days}d ${hours}h ${mins}m`;
  }
  if (hours > 0) {
    return `${hours}h ${mins}m`;
  }
  return `${mins}m ${rem}s`;
}

function isFutureIso(iso: string | null): boolean {
  if (iso === null) {
    return false;
  }
  return Date.parse(iso) > Date.now();
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== undefined && className !== "") {
    node.className = className;
  }
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
}

function makeBtn(
  label: string,
  onClick: () => void,
  opts?: { readonly primary?: boolean; readonly active?: boolean; readonly small?: boolean; readonly disabled?: boolean },
): HTMLButtonElement {
  const b = el("button", "classic-btn", label);
  b.type = "button";
  if (opts?.primary) {
    b.classList.add("primary");
  }
  if (opts?.active) {
    b.classList.add("active-toggle");
  }
  if (opts?.small) {
    b.classList.add("btn-sm");
  }
  b.disabled = opts?.disabled ?? false;
  b.addEventListener("click", onClick);
  return b;
}

export function mountAdminPage(root: HTMLElement): void {
  document.title = "Boomergames — Admin Console";

  let googleClientId: string | null = null;
  let token: string | null = readAdminToken();
  let overview: AdminOverview | null = null;
  let activeTab: AdminTab = "overview";
  let userFilter = "";
  let activityFilter: ActivityFilter = "all";
  let statusNotice: { readonly kind: "ok" | "error"; readonly text: string } | null = null;
  let gateError: string | null = null;
  let loading = true;
  let autoRefresh = false;
  let refreshTimer: number | null = null;
  let promptDraft: string | null = null;

  const setAutoRefresh = (enabled: boolean): void => {
    autoRefresh = enabled;
    if (refreshTimer !== null) {
      window.clearInterval(refreshTimer);
      refreshTimer = null;
    }
    if (enabled && overview !== null) {
      refreshTimer = window.setInterval(() => {
        void loadOverview(false);
      }, 15_000);
    }
  };

  const apiFetch = async (path: string, method: "GET" | "POST" = "GET", body?: unknown): Promise<Response> => {
    const headers: Record<string, string> = { Accept: "application/json" };
    if (token !== null) {
      headers.Authorization = `Bearer ${token}`;
    }
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }
    return fetch(path, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      cache: "no-store",
    });
  };

  const loadOverview = async (showSpinner = true): Promise<void> => {
    if (showSpinner) {
      loading = true;
      render();
    }
    try {
      if (googleClientId === null) {
        const stResp = await fetch("/status", { cache: "no-store" });
        if (stResp.ok) {
          const st = (await stResp.json()) as { googleClientId?: unknown };
          if (typeof st.googleClientId === "string" && st.googleClientId.trim() !== "") {
            googleClientId = st.googleClientId.trim();
          }
        }
      }
      if (token === null) {
        overview = null;
        loading = false;
        render();
        return;
      }
      const resp = await apiFetch("/antonadmin/api/overview");
      if (resp.status === 403 || resp.status === 401) {
        overview = null;
        token = null;
        writeAdminToken(null);
        setAutoRefresh(false);
        gateError = "403 Forbidden — Google sign-in as antonrechenauer@gmail.com is required.";
      } else if (resp.ok) {
        overview = (await resp.json()) as AdminOverview;
        gateError = null;
        if (promptDraft === null) {
          promptDraft = overview.aiFriend.activePrompt;
        }
      } else {
        statusNotice = { kind: "error", text: `Failed to load admin overview (HTTP ${resp.status}).` };
      }
    } catch (err) {
      statusNotice = { kind: "error", text: `Network error: ${String(err)}` };
    }
    loading = false;
    render();
  };

  const exchangeGoogleToken = async (idToken: string): Promise<void> => {
    gateError = null;
    loading = true;
    render();
    try {
      const resp = await fetch("/antonadmin/api/auth/google", {
        method: "POST",
        headers: { "Content-Type": "application/json", Accept: "application/json" },
        body: JSON.stringify({ idToken }),
        cache: "no-store",
      });
      if (!resp.ok) {
        token = null;
        writeAdminToken(null);
        overview = null;
        gateError = "403 Forbidden — Access is strictly restricted to Google-authenticated antonrechenauer@gmail.com.";
        loading = false;
        render();
        return;
      }
      const data = (await resp.json()) as { token: string };
      token = data.token;
      writeAdminToken(data.token);
      statusNotice = { kind: "ok", text: "Authenticated via Google." };
      await loadOverview(false);
    } catch (err) {
      gateError = `Authentication failed: ${String(err)}`;
      loading = false;
      render();
    }
  };

  const postAction = async (path: string, body: unknown, okMessage: string): Promise<void> => {
    statusNotice = null;
    try {
      const resp = await apiFetch(path, "POST", body);
      if (resp.status === 403) {
        overview = null;
        token = null;
        writeAdminToken(null);
        gateError = "Session expired or unauthorized (403 Forbidden). Please sign in with Google again.";
        render();
        return;
      }
      if (!resp.ok) {
        const errData = (await resp.json().catch(() => ({}))) as { error?: string };
        statusNotice = { kind: "error", text: errData.error ?? `Action failed (HTTP ${resp.status}).` };
        render();
        return;
      }
      statusNotice = { kind: "ok", text: okMessage };
      await loadOverview(false);
    } catch (err) {
      statusNotice = { kind: "error", text: `Request error: ${String(err)}` };
      render();
    }
  };

  const renderGate = (): void => {
    root.innerHTML = "";
    const wrap = el("main", "admin-gate-wrap");
    const card = el("section", "game-card admin-gate-card");
    const title = el("h1", "brand-title", "403 Forbidden — Restricted Area");
    const desc = el(
      "p",
      "lounge-empty",
      "This page is forbidden for everyone except the site owner authenticated via Google (Gmail).",
    );
    card.append(title, desc);

    if (gateError !== null) {
      const err = el("div", "error-banner", gateError);
      err.setAttribute("role", "alert");
      card.append(err);
    }

    if (loading) {
      card.append(el("p", "", "Checking authorization…"));
    } else if (googleClientId === null) {
      card.append(
        el(
          "div",
          "error-banner",
          "Google Sign-In is not configured on this server (GOOGLE_CLIENT_ID unset).",
        ),
      );
    } else {
      const gsiSlot = el("div", "google-signin-slot", "Loading Google Sign-In…");
      card.append(gsiSlot);
      const clientId = googleClientId;
      void loadGoogleIdentityScript().then((gsi) => {
        if (!gsiSlot.isConnected) {
          return;
        }
        if (gsi === null) {
          gsiSlot.textContent = "Could not load Google Sign-In script.";
          return;
        }
        gsiSlot.textContent = "";
        gsi.initialize({
          client_id: clientId,
          callback: (resp) => {
            if (typeof resp.credential === "string" && resp.credential !== "") {
              void exchangeGoogleToken(resp.credential);
            }
          },
        });
        gsi.renderButton(gsiSlot, { theme: "outline", size: "large", width: 280 });
      });
    }

    const backLink = el("a", "classic-btn", "← Return to Boomergames");
    backLink.href = "/";
    backLink.style.textDecoration = "none";
    backLink.style.textAlign = "center";
    card.append(backLink);

    wrap.append(card);
    root.append(wrap);
  };

  const renderOverviewTab = (data: AdminOverview): HTMLElement => {
    const container = el("div", "admin-section-stack");

    // 1. Maintenance Mode Control Panel
    const maintPanel = el("section", "lounge-panel");
    const maintHeader = el("div", "panel-heading-row");
    maintHeader.append(
      el("h2", "", "Maintenance Mode (Start / Stop Site)"),
      el(
        "span",
        data.server.maintenance ? "admin-badge admin-badge-danger" : "admin-badge admin-badge-ok",
        data.server.maintenance ? "MAINTENANCE ACTIVE (SITE STOPPED)" : "SITE LIVE",
      ),
    );
    maintPanel.append(maintHeader);

    const maintHint = el(
      "p",
      "lounge-empty",
      "When enabled, visitors receive a 503 Maintenance page and WebSockets are refused. /antonadmin and /status remain accessible so you can resume anytime.",
    );
    const msgInput = el("input", "classic-input");
    msgInput.type = "text";
    msgInput.maxLength = 300;
    msgInput.value = data.server.maintenanceMessage;
    msgInput.placeholder = "Maintenance banner message shown to visitors…";

    const kickRow = el("label", "checkbox-row");
    const kickBox = document.createElement("input");
    kickBox.type = "checkbox";
    kickBox.checked = true;
    kickRow.append(kickBox, document.createTextNode("Immediately disconnect active online sessions when stopping site"));

    const maintBtns = el("div", "control-row");
    if (data.server.maintenance) {
      maintBtns.append(
        makeBtn(
          "▶ Resume Site (Disable Maintenance Mode)",
          () => {
            void postAction(
              "/antonadmin/api/maintenance",
              { enabled: false, message: msgInput.value, disconnectClients: false },
              "Maintenance Mode disabled — site is LIVE again!",
            );
          },
          { primary: true },
        ),
        makeBtn("Update Banner Message", () => {
          void postAction(
            "/antonadmin/api/maintenance",
            { enabled: true, message: msgInput.value, disconnectClients: false },
            "Maintenance banner message updated.",
          );
        }),
      );
    } else {
      maintBtns.append(
        makeBtn(
          "⛔ Stop Site (Enable Maintenance Mode)",
          () => {
            if (window.confirm("Put Boomergames into Maintenance Mode now?")) {
              void postAction(
                "/antonadmin/api/maintenance",
                { enabled: true, message: msgInput.value, disconnectClients: kickBox.checked },
                "Maintenance Mode enabled — site stopped for regular visitors.",
              );
            }
          },
          { primary: true },
        ),
      );
    }
    maintPanel.append(maintHint, msgInput, kickRow, maintBtns);
    container.append(maintPanel);

    // 2. Broadcast System Announcement
    const bcPanel = el("section", "lounge-panel");
    bcPanel.append(el("h2", "", "Broadcast Announcement to Online Players"));
    const bcForm = el("form", "chat-form");
    const bcInput = el("input", "classic-input");
    bcInput.type = "text";
    bcInput.maxLength = 300;
    bcInput.placeholder = "e.g., Quick server update in 5 minutes — finish your current game!";
    const bcSubmit = el("button", "classic-btn primary", "Send Broadcast");
    bcSubmit.type = "submit";
    bcForm.append(bcInput, bcSubmit);
    bcForm.addEventListener("submit", (ev) => {
      ev.preventDefault();
      const text = bcInput.value.trim();
      if (text === "") {
        return;
      }
      void postAction("/antonadmin/api/broadcast", { message: text }, "Broadcast delivered to all online clients.");
      bcInput.value = "";
    });
    bcPanel.append(bcForm);
    container.append(bcPanel);

    // 3. Live Online Sessions & Active Tables
    const liveGrid = el("div", "home-grid");

    const sessPanel = el("section", "lounge-panel");
    sessPanel.append(el("h2", "", `Currently Online Sessions (${data.onlineSessions.length})`));
    if (data.onlineSessions.length === 0) {
      sessPanel.append(el("p", "lounge-empty", "No WebSocket clients currently connected."));
    } else {
      const wrap = el("div", "admin-table-wrap");
      const tbl = el("table", "admin-table");
      tbl.innerHTML = "<thead><tr><th>Wire ID</th><th>Type</th><th>Player</th><th>Active Table</th></tr></thead>";
      const tb = document.createElement("tbody");
      for (const s of data.onlineSessions) {
        const tr = document.createElement("tr");
        const c1 = el("td", "admin-mono", s.wireId);
        const c2 = el("td", "", s.kind);
        const c3 = el("td", "", s.email !== null ? `${s.displayName} (${s.email})` : s.displayName);
        const c4 = el("td", "admin-mono", s.activeTable ?? "—");
        tr.append(c1, c2, c3, c4);
        tb.append(tr);
      }
      tbl.append(tb);
      wrap.append(tbl);
      sessPanel.append(wrap);
    }

    const roomsPanel = el("section", "lounge-panel");
    roomsPanel.append(el("h2", "", `Live Rooms (${data.rooms.length}) & Active Tables (${data.activeTables.length})`));
    const roomList = el("div", "control-row");
    for (const r of data.rooms) {
      roomList.append(el("span", "admin-badge", `${r.name}: ${r.users} users`));
    }
    roomsPanel.append(roomList);

    if (data.activeTables.length > 0) {
      const wrap = el("div", "admin-table-wrap");
      const tbl = el("table", "admin-table");
      tbl.innerHTML = "<thead><tr><th>Table</th><th>Room</th><th>Action</th></tr></thead>";
      const tb = document.createElement("tbody");
      for (const t of data.activeTables) {
        const tr = document.createElement("tr");
        const c1 = el("td", "admin-mono", t.tableId);
        const c2 = el("td", "admin-mono", t.roomId);
        const c3 = document.createElement("td");
        c3.append(
          makeBtn(
            "Force Close",
            () => {
              if (window.confirm(`Force close table ${t.tableId}?`)) {
                void postAction("/antonadmin/api/tables/close", { tableId: t.tableId }, `Closed table ${t.tableId}.`);
              }
            },
            { small: true },
          ),
        );
        tr.append(c1, c2, c3);
        tb.append(tr);
      }
      tbl.append(tb);
      wrap.append(tbl);
      roomsPanel.append(wrap);
    }

    liveGrid.append(sessPanel, roomsPanel);
    container.append(liveGrid);

    return container;
  };

  const renderUsersTab = (data: AdminOverview): HTMLElement => {
    const panel = el("section", "lounge-panel");
    const headerRow = el("div", "panel-heading-row");
    headerRow.append(el("h2", "", `Registered Users (${data.users.length})`));
    const search = el("input", "classic-input admin-search");
    search.type = "search";
    search.placeholder = "Filter by name or email…";
    search.value = userFilter;
    search.addEventListener("input", () => {
      userFilter = search.value;
      render();
      const refreshed = root.querySelector<HTMLInputElement>(".admin-search");
      if (refreshed !== null) {
        refreshed.focus();
        refreshed.setSelectionRange(userFilter.length, userFilter.length);
      }
    });
    headerRow.append(search);
    panel.append(headerRow);

    const q = userFilter.trim().toLowerCase();
    const filtered = data.users.filter(
      (u) => q === "" || u.displayName.toLowerCase().includes(q) || u.email.toLowerCase().includes(q),
    );

    if (filtered.length === 0) {
      panel.append(el("p", "lounge-empty", "No matching users found."));
      return panel;
    }

    const wrap = el("div", "admin-table-wrap");
    const tbl = el("table", "admin-table");
    tbl.innerHTML =
      "<thead><tr><th>ID</th><th>User</th><th>Email & Auth</th><th>Joined</th><th>Last Login</th><th>Stats</th><th>Status & Actions</th></tr></thead>";
    const tb = document.createElement("tbody");

    for (const u of filtered) {
      const tr = document.createElement("tr");
      const isSelf = u.email.toLowerCase() === ADMIN_EMAIL;
      const muted = isFutureIso(u.mutedUntil);
      const banned = isFutureIso(u.bannedUntil);

      const tdId = el("td", "admin-mono", `#${u.id}`);

      const tdUser = document.createElement("td");
      tdUser.append(el("strong", "", u.displayName));
      if (u.online) {
        tdUser.append(document.createTextNode(" "), el("span", "admin-badge admin-badge-ok", "ONLINE"));
      }
      if (u.isModerator) {
        tdUser.append(document.createTextNode(" "), el("span", "admin-badge", "MOD"));
      }
      if (isSelf) {
        tdUser.append(document.createTextNode(" "), el("span", "admin-badge admin-badge-ok", "ADMIN"));
      }

      const tdEmail = document.createElement("td");
      tdEmail.append(el("div", "admin-mono", u.email));
      const authTag = u.googleLinked ? "Google linked" : "Email only";
      tdEmail.append(el("div", "roster-sub", authTag));

      const tdJoined = el("td", "admin-mono", formatDateTime(u.createdAt));

      const tdLogin = document.createElement("td");
      tdLogin.append(el("div", "admin-mono", formatDateTime(u.lastLoginAt)));
      if (u.lastLoginProvider !== null) {
        tdLogin.append(el("div", "roster-sub", `via ${u.lastLoginProvider}`));
      }

      const tdStats = document.createElement("td");
      if (u.stats.length === 0) {
        tdStats.textContent = "—";
      } else {
        for (const st of u.stats) {
          tdStats.append(el("div", "roster-sub", `${st.game}: ${st.rating} Elo (${st.wins}W / ${st.games}G)`));
        }
      }

      const tdAct = document.createElement("td");
      const statusRow = el("div", "control-row");
      if (banned) {
        statusRow.append(el("span", "admin-badge admin-badge-danger", `Banned until ${formatDateTime(u.bannedUntil)}`));
      }
      if (muted) {
        statusRow.append(el("span", "admin-badge admin-badge-warn", `Muted until ${formatDateTime(u.mutedUntil)}`));
      }
      if (statusRow.childElementCount > 0) {
        tdAct.append(statusRow);
      }

      const actRow = el("div", "control-row");
      actRow.append(
        makeBtn(
          muted ? "Unmute" : "Mute 1h",
          () => {
            void postAction(
              "/antonadmin/api/users/mute",
              { userId: u.id, minutes: muted ? 0 : 60 },
              muted ? `Unmuted ${u.displayName}.` : `Muted ${u.displayName} for 1 hour.`,
            );
          },
          { small: true },
        ),
        makeBtn(
          banned ? "Unban" : "Ban 24h",
          () => {
            if (!banned && !window.confirm(`Ban ${u.displayName} (${u.email}) for 24 hours?`)) {
              return;
            }
            void postAction(
              "/antonadmin/api/users/ban",
              { userId: u.id, hours: banned ? 0 : 24 },
              banned ? `Unbanned ${u.displayName}.` : `Banned ${u.displayName} for 24 hours.`,
            );
          },
          { small: true, disabled: isSelf && !banned },
        ),
        makeBtn(
          u.isModerator ? "Remove Mod" : "Make Mod",
          () => {
            void postAction(
              "/antonadmin/api/users/moderator",
              { userId: u.id, isModerator: !u.isModerator },
              `Updated moderator flag for ${u.displayName}.`,
            );
          },
          { small: true },
        ),
        makeBtn(
          "Delete",
          () => {
            if (window.confirm(`PERMANENTLY delete user ${u.displayName} (${u.email}) and all stats?`)) {
              void postAction("/antonadmin/api/users/delete", { userId: u.id }, `Deleted user ${u.displayName}.`);
            }
          },
          { small: true, disabled: isSelf },
        ),
      );
      tdAct.append(actRow);

      tr.append(tdId, tdUser, tdEmail, tdJoined, tdLogin, tdStats, tdAct);
      tb.append(tr);
    }
    tbl.append(tb);
    wrap.append(tbl);
    panel.append(wrap);
    return panel;
  };

  const renderActivityTab = (data: AdminOverview): HTMLElement => {
    const panel = el("section", "lounge-panel");
    const headerRow = el("div", "panel-heading-row");
    headerRow.append(el("h2", "", `Logins, Anonymous Visits & Offline Plays (${data.activity.length})`));

    const filters = el("div", "control-row");
    const filterDefs: readonly [ActivityFilter, string][] = [
      ["all", "All"],
      ["logins", "Logins"],
      ["anonymous", "Guests / Anonymous"],
      ["offline", "Offline Plays"],
      ["admin", "Admin Actions"],
    ];
    for (const [key, label] of filterDefs) {
      filters.append(
        makeBtn(
          label,
          () => {
            activityFilter = key;
            render();
          },
          { active: activityFilter === key, small: true },
        ),
      );
    }
    headerRow.append(filters);
    panel.append(headerRow);

    const rows = data.activity.filter((ev) => {
      if (activityFilter === "all") {
        return true;
      }
      if (activityFilter === "logins") {
        return ev.eventType === "login_google" || ev.eventType === "login_email" || ev.eventType === "session_resume";
      }
      if (activityFilter === "anonymous") {
        return ev.eventType === "guest_connect" || ev.eventType === "offline_play";
      }
      if (activityFilter === "offline") {
        return ev.eventType === "offline_play";
      }
      return ev.eventType === "admin_action";
    });

    if (rows.length === 0) {
      panel.append(el("p", "lounge-empty", "No activity events recorded for this filter yet."));
      return panel;
    }

    const wrap = el("div", "admin-table-wrap");
    const tbl = el("table", "admin-table");
    tbl.innerHTML =
      "<thead><tr><th>Date & Time</th><th>Event</th><th>Actor</th><th>Provider</th><th>Game</th><th>Details</th></tr></thead>";
    const tb = document.createElement("tbody");
    for (const ev of rows) {
      const tr = document.createElement("tr");
      tr.append(
        el("td", "admin-mono", formatDateTime(ev.createdAt)),
        (() => {
          const td = document.createElement("td");
          td.append(el("span", "admin-badge", ev.eventType));
          return td;
        })(),
        el("td", "", ev.actorLabel),
        el("td", "", ev.authProvider ?? "—"),
        el("td", "", ev.game ?? "—"),
        el("td", "", ev.details ?? "—"),
      );
      tb.append(tr);
    }
    tbl.append(tb);
    wrap.append(tbl);
    panel.append(wrap);
    return panel;
  };

  const renderMatchesReportsTab = (data: AdminOverview): HTMLElement => {
    const stack = el("div", "admin-section-stack");

    // Moderation Reports
    const repPanel = el("section", "lounge-panel");
    repPanel.append(el("h2", "", `Moderation Reports (${data.reports.length}, ${data.server.openReports} open)`));
    if (data.reports.length === 0) {
      repPanel.append(el("p", "lounge-empty", "No moderation reports submitted."));
    } else {
      const wrap = el("div", "admin-table-wrap");
      const tbl = el("table", "admin-table");
      tbl.innerHTML =
        "<thead><tr><th>ID</th><th>Date & Time</th><th>Reporter</th><th>Reported</th><th>Reason & Chat</th><th>Status</th></tr></thead>";
      const tb = document.createElement("tbody");
      for (const r of data.reports) {
        const tr = document.createElement("tr");
        const reasonTd = document.createElement("td");
        reasonTd.append(el("div", "", r.reason));
        if (r.chatLog !== null && r.chatLog !== "") {
          const details = document.createElement("details");
          details.append(el("summary", "", "View chat snapshot"));
          details.append(el("pre", "admin-chat-pre", r.chatLog));
          reasonTd.append(details);
        }
        const statusTd = document.createElement("td");
        if (r.resolvedAt !== null) {
          statusTd.append(el("span", "admin-badge admin-badge-ok", `Resolved ${formatDateTime(r.resolvedAt)}`));
        } else {
          statusTd.append(
            makeBtn(
              "Resolve",
              () => {
                void postAction("/antonadmin/api/reports/resolve", { reportId: r.id }, `Resolved report #${r.id}.`);
              },
              { small: true, primary: true },
            ),
          );
        }
        tr.append(
          el("td", "admin-mono", `#${r.id}`),
          el("td", "admin-mono", formatDateTime(r.createdAt)),
          el("td", "", r.reporterName ?? "Guest"),
          el("td", "", r.reportedName),
          reasonTd,
          statusTd,
        );
        tb.append(tr);
      }
      tbl.append(tb);
      wrap.append(tbl);
      repPanel.append(wrap);
    }
    stack.append(repPanel);

    // Recent Matches
    const matchPanel = el("section", "lounge-panel");
    matchPanel.append(el("h2", "", `Recent Online Matches (showing ${data.recentMatches.length} of ${data.server.totalMatches})`));
    if (data.recentMatches.length === 0) {
      matchPanel.append(el("p", "lounge-empty", "No finished online matches recorded yet."));
    } else {
      const wrap = el("div", "admin-table-wrap");
      const tbl = el("table", "admin-table");
      tbl.innerHTML =
        "<thead><tr><th>Ended At</th><th>Game</th><th>Matchup</th><th>Winner</th><th>Reason</th><th>Rated</th></tr></thead>";
      const tb = document.createElement("tbody");
      for (const m of data.recentMatches) {
        const winnerLabel =
          m.winner === 1 ? m.player1Label : m.winner === 2 ? m.player2Label : "Draw";
        const tr = document.createElement("tr");
        tr.append(
          el("td", "admin-mono", formatDateTime(m.endedAt)),
          el("td", "", m.game),
          el("td", "", `${m.player1Label} vs ${m.player2Label}`),
          el("td", "", winnerLabel),
          el("td", "", m.endReason),
          el("td", "", m.rated ? "Rated" : "Unrated"),
        );
        tb.append(tr);
      }
      tbl.append(tb);
      wrap.append(tbl);
      matchPanel.append(wrap);
    }
    stack.append(matchPanel);

    return stack;
  };

  const renderAiTab = (data: AdminOverview): HTMLElement => {
    const panel = el("section", "lounge-panel");
    const ai = data.aiFriend;
    const head = el("div", "panel-heading-row");
    head.append(
      el("h2", "", "Claude AI Friend Companion"),
      el(
        "span",
        ai.configured ? "admin-badge admin-badge-ok" : "admin-badge admin-badge-warn",
        ai.configured ? `ACTIVE (${ai.model})` : "NOT CONFIGURED",
      ),
    );
    panel.append(head);

    if (!ai.configured) {
      panel.append(el("p", "lounge-empty", "ANTHROPIC_API_KEY is not configured on this server."));
      return panel;
    }

    const metaRow = el("div", "control-row");
    metaRow.append(
      el("span", "admin-badge", `Calls used: ${ai.callsUsed} / ${ai.maxCalls}`),
      el("span", "admin-badge", `Source: ${ai.usingCustomFile ? "Custom file" : "Built-in default"}`),
      el("span", "admin-mono", `File: ${ai.promptFile ?? "—"}`),
      makeBtn(
        "Reset Call Counter to 0",
        () => {
          void postAction("/antonadmin/api/ai/reset", {}, "AI Friend safety call counter reset to 0.");
        },
        { small: true },
      ),
    );
    panel.append(metaRow);

    const textarea = el("textarea", "classic-input admin-prompt-editor");
    textarea.rows = 10;
    textarea.value = promptDraft ?? ai.activePrompt;
    textarea.addEventListener("input", () => {
      promptDraft = textarea.value;
    });

    const saveRow = el("div", "control-row");
    saveRow.append(
      makeBtn(
        "Save System Prompt to Disk (Takes Effect Immediately)",
        () => {
          const val = textarea.value.trim();
          if (val === "") {
            statusNotice = { kind: "error", text: "System prompt cannot be empty." };
            render();
            return;
          }
          void postAction("/antonadmin/api/ai/prompt", { prompt: val }, "AI Friend system prompt saved to disk!");
        },
        { primary: true },
      ),
      makeBtn("Reload from Server", () => {
        promptDraft = null;
        void loadOverview(false);
      }),
    );

    panel.append(textarea, saveRow);
    return panel;
  };

  const render = (): void => {
    if (overview === null) {
      renderGate();
      return;
    }

    const data = overview;
    root.innerHTML = "";

    const header = el("header", "top-bar");
    const brand = el("div", "brand");
    brand.append(
      el("h1", "brand-title", "Boomergames Admin"),
      el(
        "span",
        "brand-badge",
        data.server.maintenance ? "MAINTENANCE" : `LIVE v${data.server.version}`,
      ),
      el("span", "roster-sub", data.adminEmail),
    );
    const controls = el("div", "top-controls");
    controls.append(
      makeBtn("↻ Refresh", () => void loadOverview(false)),
      makeBtn(
        autoRefresh ? "Auto-Refresh: On (15s)" : "Auto-Refresh: Off",
        () => {
          setAutoRefresh(!autoRefresh);
          render();
        },
        { active: autoRefresh },
      ),
      makeBtn("Open Site ↗", () => {
        window.open("/", "_blank", "noopener");
      }),
      makeBtn("Sign Out Admin", () => {
        token = null;
        writeAdminToken(null);
        overview = null;
        setAutoRefresh(false);
        render();
      }),
    );
    header.append(brand, controls);
    root.append(header);

    const main = el("main", "main-area admin-shell");

    if (statusNotice !== null) {
      const banner = el(
        "div",
        statusNotice.kind === "error" ? "error-banner" : "notice-banner",
        statusNotice.text,
      );
      main.append(banner);
    }

    // KPI Cards
    const kpiGrid = el("section", "admin-kpi-grid");
    const kpis: readonly [string, string][] = [
      ["Site Status", data.server.maintenance ? "⛔ Maintenance" : "✅ Live"],
      ["Uptime", formatUptime(data.server.uptimeS)],
      ["Database", data.server.dbOk ? "✅ OK" : "❌ Error"],
      ["Online Now", String(data.server.online)],
      ["Active Tables", String(data.server.tables)],
      ["Running Games", String(data.server.games)],
      ["Registered Users", String(data.server.totalUsers)],
      ["Total Matches", String(data.server.totalMatches)],
      ["Open Reports", String(data.server.openReports)],
      [
        "AI Friend Calls",
        data.aiFriend.configured ? `${data.aiFriend.callsUsed} / ${data.aiFriend.maxCalls}` : "Off",
      ],
    ];
    for (const [label, val] of kpis) {
      const card = el("div", "admin-kpi-card");
      card.append(el("div", "admin-kpi-label", label), el("div", "admin-kpi-value", val));
      kpiGrid.append(card);
    }
    main.append(kpiGrid);

    // Tab Navigation
    const tabs = el("nav", "admin-tabs");
    const tabDefs: readonly [AdminTab, string][] = [
      ["overview", "Overview & Maintenance"],
      ["users", `Users (${data.users.length})`],
      ["activity", `Logins & Plays (${data.activity.length})`],
      ["matches", `Matches & Reports (${data.server.openReports})`],
      ["ai", "AI Friend"],
    ];
    for (const [id, label] of tabDefs) {
      tabs.append(
        makeBtn(
          label,
          () => {
            activeTab = id;
            render();
          },
          { active: activeTab === id },
        ),
      );
    }
    main.append(tabs);

    switch (activeTab) {
      case "overview":
        main.append(renderOverviewTab(data));
        break;
      case "users":
        main.append(renderUsersTab(data));
        break;
      case "activity":
        main.append(renderActivityTab(data));
        break;
      case "matches":
        main.append(renderMatchesReportsTab(data));
        break;
      case "ai":
        main.append(renderAiTab(data));
        break;
    }

    root.append(main);
  };

  void loadOverview(true);
}

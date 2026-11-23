# Boomergames — Product Plan

*Version 0.8 · 23 Nov 2026 · Status: Draft*

**Product name:** Boomergames (confirmed). **Completely free:** no ads, no purchases, no money involved.

**Sources:** `turn_based_multiplayer_game_platform_requirements.md` (URD), `yahoo_pool_historical_overview.pdf`, the Black Label Billiards article "Looking Back at Yahoo Pool", and two screenshots of the classic client (EN "Pool" and JP "Billiards").

---

## 1. Vision

Build a browser-based **lounge for classic games** that brings back the feel of the old Yahoo! Games rooms: you walk into a room, see who's there, chat, join a table or watch one, and play a quick turn-based match against a real person. Pool (8-ball) and Checkers come first.

Players remembered the **community** more than the graphics. As one article commenter put it: *"it was just a great place to hang out whether you got in a game or not. The lobby aspect of gaming is so lost on this generation."* Social features (rooms, rosters, table chat, spectating, ratings) are the core product. They aren't extras.

**What we're not building:** a Miniclip-style app with loot boxes, cue upgrades, energy systems or pay-to-win mechanics.

**IP note:** The game is *inspired by* the Yahoo Games experience and recreates its **era and feel** (see §6.4). Its name, logo, art and sounds are our own. We don't copy Yahoo branding, screenshots or assets one-to-one. Game rules (8-ball, checkers) and standard pool-table conventions (green cloth, wood rails, standard ball colours) are public domain.

---

## 2. Target users

| Persona | Description | What they need |
|---|---|---|
| **Nostalgic regular** (core) | 35–65, played Yahoo/MSN/Pogo games in the 2000s. Plays on an office break or in the evening. | Fast entry, familiar lobby → table flow, readable UI, ratings that mean something, regular faces in the rooms |
| **Casual drop-in** | Arrives from a link a friend sent | Play within 30 s, guest access, no install |
| **Competitive player** | Wants rated matches, tournaments, leaderboards | Fair, deterministic physics, cheat resistance, match history |
| **Spectator / socialiser** | Hangs out, chats, watches good players | Watch any public table, chat, low friction |

**Accessibility baseline:** this audience is older, so we use large readable text (≥16 px base and scalable), high-contrast mode, no reliance on colour alone (stripes vs. solids also get a pattern or label), full keyboard support for menus, and generous click targets.

---

## 3. Scope

### 3.1 MVP (Phase 1)
- Accounts (registered + guest)
- Lounges (rooms) with live roster and room chat
- Table list, create/join/watch, Play Now quick match, direct invites
- Game room with table chat, spectators, turn timer, ready/start, rematch
- **Game 1: Pool — 8-ball**
- **Game 2: Checkers (English draughts, 8×8)**
- **Computer opponents** for both games, so nobody ever faces an empty room (see §5.6)
- Ratings (per game) and basic profile with match history
- **High score table** (wins per player, per game; see §5.7)
- Disconnect/reconnect handling, resign
- Basic moderation: mute/ignore, report, host boot, chat rate limit, profanity filter

### 3.2 Later (Phase 2+)
- 9-ball, other pool variants; international/German draughts (10×10 / flying kings)
- Friends list and "follow", online notifications
- Tournaments (bracket, room-hosted), seasonal/monthly leaderboards
- More games on the same platform (chess, backgammon, reversi, dominoes, …)
- Native app wrappers (PWA first, then Capacitor if needed)

### 3.3 Out of scope (per URD)
Real-time twitch games and voice/video chat.

**Never in scope:** any form of monetisation. That means no payments, in-app purchases, premium accounts, ads, sponsored content, donations, or selling user data. Boomergames is and stays completely free. Every feature is available to every player.

---

## 4. User roles

As defined in URD §1.3: **Guest**, **Registered Player (in lounge)**, **Active Player (seated)**, which is either the *Active Turn Player* or the *Waiting Player*, and **Spectator**. We add two roles:

- **Table Host:** the player who created the table (Seat 1 by default). Can set table type, invite and boot. *(The classic client had Invite and Boot buttons, and the JP screenshot shows the message "You are the host of this table. You can invite or boot players…")*
- **Moderator / Admin:** can mute or ban users, close tables and review reports.

---

## 5. Functional requirements — Platform (shared by all games)

These follow the URD (FR-1 … FR-8). Below is a summary plus our **additions and decisions**. Items marked **[new]** are not in the URD.

### 5.1 Accounts & identity
- **A-1 [new]** Guests can play unrated games straight away with an auto-generated name (`Guest_4821`). They're prompted to register to keep a rating and appear in the high score table.
- **A-1a** Guests can chat in rooms and at tables, with the same rate limit and filter as registered users (L-3). Moderators can mute or ban guests by session/IP.
- **A-2 [new]** Users sign in with an emailed **magic link** or with **Google sign-in**. There are no passwords, so there are no password resets and no password hashes to store. No phone number is required.
- **A-3 [new]** Display names must be unique and pass a profanity filter. Players can change their name once every 30 days.
- **A-4 [new]** Players pick a simple avatar from a built-in set (original art). Custom uploads are out of scope for MVP.
- **A-5 [new]** The profile shows rating per game, games played, W/L/D, last 20 matches and member-since date.

### 5.2 Lounges / rooms (URD FR-1, FR-2)
- **L-1** Lobby per game (Pool, Checkers). Each lobby has several named rooms grouped by tier: *Beginner, Intermediate, Advanced, Social*. Each room has a capacity (e.g. 100 users).
- **L-2** Live roster showing name, rating/badge and status (`Available`, `At Table – Waiting`, `In Game`, `Spectating`).
- **L-3** Room chat with a rate limit of 1 msg/s, a 300-character maximum and a profanity filter.
- **L-4** Ignore/Mute per user hides that user's chat and invites everywhere. The setting persists on the account.
- **L-5 [new]** System messages appear in chat: joins/leaves (can be toggled off) and rating changes after a game (*"X's old rating: 1727; new rating: 1711"*, as in the classic client).

### 5.3 Tables & matchmaking (URD FR-3 … FR-5)
- **T-1** The table list shows table #, Seat 1/Seat 2 (name + rating), Public/Private, rules (game variant, timer), spectator count and a Join/Watch/Full button.
- **T-2** Create Table puts the creator in Seat 1 as Host. The host configures:
  - Table type: **Public** (anyone can sit), **Protected** (anyone can watch, only invitees can sit), or **Private** (invite only, hidden from the list). *(The URD has Public/Private only. "Protected" matches the classic behaviour.)*
  - Rated / Unrated
  - Turn timer: 20 s / 30 s / 60 s / none *(the archived lobby showed "no timer / 20 sec / 30 sec")*
  - Game-specific options (see §6, §7)
  - Mute spectators on/off
- **T-3** Join an open seat on a Public table.
- **T-4** **Play Now** seats the player at an open public rated table within ±200 rating (the window widens over time). If no table is found, it creates one.
- **T-5** Invite from the roster or from inside a table. Invites expire after 30 s. Accepting creates or joins a private table (URD FR-5).
- **T-6 [new] Host controls:** Boot (removes a player or spectator, who can't rejoin that table for 5 min), Invite, change table type before the match starts.
- **T-7 [new] Stand Up:** a seated player can vacate their seat between games. Mid-game, standing up counts as a resignation.
- **T-8 [new] Shareable table link:** a URL to a table that friends can open directly. Guests landing on it are seated or spectate.

### 5.4 Game room (URD FR-6 … FR-8)
- **G-1** Layout: game board centred, player panels with turn indicator and timer above, controls to the side, table chat + occupant list below. The layout is our own design, but it keeps the familiar spatial arrangement.
- **G-2** Table chat is visible to seats and spectators. Players can chat at any time, including during the opponent's turn.
- **G-3** Lifecycle: `WAITING → READY → IN_PROGRESS → GAME_OVER → (Rematch | WAITING)`, plus `PAUSED_DISCONNECT`.
- **G-4** Both players press "Start/Ready" to begin. The first move is random; rematches alternate who starts.
- **G-5** Strict turn enforcement: the waiting player's controls are locked, a "Waiting for *Name*…" indicator is shown, and the server rejects out-of-turn input.
- **G-6** Turn timer. Expiry behaviour is set per game. Pool: the turn passes and the opponent gets **ball-in-hand**, with no match forfeit for repeated timeouts. Checkers: the player loses on time.
- **G-7** Disconnects get a 45 s grace period. On reconnect the player receives a full snapshot. If the grace period expires, they forfeit.
- **G-8** Resign button with confirmation. **Offer Draw** in checkers.
- **G-9** After the game: a result screen, rating delta, Rematch (both players must click), and Leave.
- **G-10 [new]** Sound on/off, and **Spectators: show/hide chat**.
- **G-11 [new] Abandonment protection:** a player who leaves rated games repeatedly gets a temporary matchmaking cooldown.

### 5.5 Ratings
- **R-1** Separate ratings per game. Everyone starts at 1200. Unrated and guest games don't change ratings.
- **R-2** Rating badges or tiers shown in the roster (e.g. Novice < 1200 ≤ Club < 1500 ≤ Expert < 1800 ≤ Master).
- **R-3** Players with fewer than 10 rated games are shown as provisional ("?" marker).
- **R-4 [new]** Anti-farming: rating gains between the same two accounts are reduced after N games per day.

### 5.6 Never an empty room [new]
Cold start is the biggest product risk: a lounge with no people in it is dead on arrival. These requirements are part of the MVP:
- **E-1 Computer opponents:** any player can start a game vs. the computer at once, from the lobby or by adding the computer to an empty seat at their table. Three levels: *Easy / Medium / Hard*.
- **E-2 House players:** each room always shows at least 2–3 open tables hosted by the computer, labelled clearly with a 🤖 badge and names like "House Bot (Medium)". Players never mistake a bot for a human.
- **E-3 Human first:** when a real player opens a seat or clicks Play Now, matchmaking first waits up to 20 s for a human, then offers "Play the computer while you wait". If a human becomes available, the player gets a non-blocking notification.
- **E-4 Bot games are unrated.** They count in a separate "vs. computer" stat on the profile.
- **E-5 Few, busy rooms:** at launch, each game gets only 1–2 rooms. New rooms open automatically when a room passes ~70 % capacity, so users gather in one place instead of spreading out.
- **E-6 Visible life:** the landing page and lobby show "*N players online now*", live tables in progress (watchable), and recent results.
- **E-7 Scheduled game nights:** a weekly "game night" time slot is shown in the lobby and can be added to a calendar, so players know when others are around.
- **E-8 Invite a friend:** a one-click table link (T-8) to share by email or WhatsApp. This is the main growth tool.

### 5.7 High score table [new]
- **H-1** Each game (Pool, Checkers) has its own **high score table** that ranks registered players by their **total number of wins**.
- **H-2** Every win against a **human** opponent counts, rated or unrated, including wins by resignation, timeout or disconnect-forfeit. **Wins against the computer never count.** Draws and losses don't count.
- **H-3** Guests don't appear on the table. Guest wins aren't carried over after registration.
- **H-4** The table shows rank, name, avatar, number of wins, and (in a small, secondary column) games played. Ties are broken by the earlier date the player reached that win count.
- **H-5** The lobby shows the Top 100 for that game. A **"Find me"** button jumps to the player's own position. The player's own rank also appears on their profile.
- **H-6** A player's position on the high score table is updated immediately after each game.
- **H-7 Anti-farming:** only the first **5 wins per day against the same opponent** count. This stops two friends (or one person with two accounts) from boosting each other. Games shorter than a minimum (e.g. pool: before the first shot after the break; checkers: fewer than 5 moves per side) don't count.
- **H-8** Banned accounts are removed from the table.

### 5.8 Moderation & safety
- **M-1** Report a user (chat abuse, cheating, stalling) and attach the last chat log automatically.
- **M-2** Moderator tools: mute (time-limited), ban, close table, view reports.
- **M-3** Server-side chat filter, configurable per room.
- **M-4** No private DMs in MVP, which reduces the harassment surface. Communication happens in rooms and at tables only.

---

## 6. Game 1 — Pool (8-ball)

### 6.1 Rules (default ruleset, "Standard 8-ball")
- **P-1** 15 balls racked in a triangle with the 8-ball in the centre. The cue ball is placed behind the head string. Break alternates in rematches.
- **P-2** The **open table** lasts until a player legally pockets a ball after the break. That ball's group (solids 1–7 / stripes 9–15) becomes theirs.
- **P-3** A player keeps shooting while they legally pocket a ball of their own group. Otherwise the turn passes.
- **P-4** **Fouls:** cue ball pocketed (scratch); wrong ball hit first; no ball hit; no ball reaches a rail after contact; cue ball leaves the table. **Penalty:** opponent gets **ball-in-hand** (anywhere on the table — configurable to "behind the head string").
- **P-5** **Win:** legally pocket the 8-ball after clearing your group. **Loss:** pocketing the 8 early, pocketing it on a foul, or knocking it off the table.
- **P-6** **Table options:** call-pocket for the 8-ball (on/off), ball-in-hand anywhere / behind line, turn timer, aiming aid level (see P-11).
- **P-7** Break rules: if the 8-ball is pocketed on the break, re-rack (or win, configurable). A scratch on the break gives ball-in-hand.

### 6.2 Controls & feel
- **P-8 Aiming:** on desktop, move the mouse to rotate the cue around the cue ball. On touch, drag to aim, with a fine-adjust slider/wheel for precision.
- **P-9 Power:** a vertical power meter beside the table (familiar from the classic client). Pull back by dragging or with a slider, then release/click to shoot.
- **P-10 Spin ("English"):** a cue-ball spin selector (top/back/left/right) as a small cue-ball widget. *(Both classic screenshots show this widget.)*
- **P-11 Aiming aid:** the aim line goes to the first contact, with a ghost ball. Optional extended lines show the object ball's path; the table setting chooses Off / Basic / Extended. *(Article commenters remember the aim line fondly. The **table host decides** the level, including at rated tables. The level is shown in the table list and in the game room, so both players know it before they sit down.)*
- **P-12 Ball-in-hand:** drag the cue ball to any legal spot. An illegal (overlapping) spot is shown in red.
- **P-13** While balls are moving, nobody can act. After they come to rest, the next turn begins (URD FR-7.4).
- **P-14** Shot clock display for the active player. A warning sound plays at 5 s.
- **P-15** The balls pocketed by each player are displayed next to their name panel.

### 6.3 Physics requirements
- **P-16** Physics must feel plausible: elastic ball-ball collisions, cushion rebound with energy loss, rolling/sliding friction, spin effect on follow/draw/side, and pocket jaws.
- **P-17** **Deterministic:** both players and all spectators see the identical outcome, every time (URD NFR-3). The server's result is authoritative.
- **P-18** The shot animation plays smoothly at 60 fps on a mid-range phone from ~2020.
- **P-19** Shot input is the only gameplay message: `{angle, power, spinX, spinY, cueBallPos?}`. The server validates the ranges.

### 6.4 Visual style — classic 2000s look
The table must immediately feel like *"the pool game from back then"*: the same era, palette and simplicity. The reference is the classic screenshots in this folder. The art itself is newly drawn, not copied pixel for pixel.
- **P-20 Perspective:** flat 2D top-down, no 3D, no perspective tilt, no lighting effects or glossy reflections. Three.js renders this with an orthographic top-down camera and unlit (flat) materials.
- **P-21 Cloth:** saturated classic billiard green (around `#2E8B2E`) with a slightly lighter playing surface and a darker inner cushion edge. No texture beyond a subtle flat shade.
- **P-22 Rails:** dark red-brown/maroon wood (around `#8B1A1A` to `#6B1010`), flat-shaded, with simple light-tan/orange diamond or corner accents.
- **P-23 Pockets:** 6 solid black round pockets with tan/orange corner and side caps.
- **P-24 Head string:** a thin, faint vertical line at the head-string position.
- **P-25 Balls:** standard pool colours (1 yellow, 2 blue, 3 red, 4 purple, 5 orange, 6 green, 7 maroon, 8 black; 9–15 the same as white-banded stripes). Each has a small white number circle and a simple highlight dot, and is drawn as a flat circle with no 3D rendering.
- **P-26 Cue:** a simple light-wood stick with a darker butt, plus a thin straight aim line from the cue ball.
- **P-27 Around the table:** a vertical power meter with tick marks on the right, a cue-ball spin widget on the left, player names above the table with a turn arrow, and a small shot timer in the centre-top.
- **P-28 Window chrome:** light, slightly dated UI (pale yellow/cream header, teal-grey side panel, plain rectangular buttons, a system-style sans-serif font such as Verdana/Tahoma), with our own Boomergames logo in a similar spirit.
- **P-29 Sounds:** simple, short, era-appropriate effects for the cue hit, ball click, cushion thud, pocket drop and join/leave chime. All are newly recorded or license-free.
- **P-30 Optional later:** a "modern" 3D theme toggle (cheap with Three.js: perspective camera + lit materials on the same scene). The classic theme stays the default.

---

## 7. Game 2 — Checkers (English draughts / American checkers)

### 7.1 Rules (default — confirmed: English/American only for MVP)
- **C-1** 8×8 board, 12 men per side on dark squares. Dark/Black moves first. Colours alternate in rematches.
- **C-2** Men move diagonally forward one square. Kings move diagonally forward or back one square (no "flying kings" in this variant).
- **C-3** **Captures are mandatory.** Multi-jumps must be completed. The player may choose which capture sequence to play when several exist (no "maximum capture" rule).
- **C-4** A man reaching the far row is crowned king. If crowning happens mid-jump, the move ends there.
- **C-5** **Win:** the opponent has no pieces or no legal moves. Opponent resigns or runs out of time.
- **C-6** **Draws:** by mutual agreement (Offer Draw), threefold repetition, or 40 moves by each side with no capture or man move.

### 7.2 Controls & feel
- **C-7** Click/tap a piece, legal destinations are highlighted, then click/tap a destination. Drag-and-drop works too.
- **C-8** If a capture is available, only the pieces that can capture are selectable, and they're highlighted. A short hint explains *"You must capture"*.
- **C-9** Multi-jumps are entered step by step, or the player clicks the final square when the path is unambiguous.
- **C-10** Last-move highlight, captured-piece count, and a move list in standard notation (e.g. `11-15`, `22x15`).
- **C-11** Optional "flip board" for spectators.
- **C-12** Timer options: per-move (20/30/60 s) or **per-game clock** (5 / 10 / 15 min). A per-game clock suits checkers better.
- **C-13** Visual style matches the pool table's classic look: flat 2D, wooden board (or classic red/black), red and black discs with a simple crown symbol for kings.

### 7.3 Later variants (Phase 2)
International draughts (10×10, flying kings, max capture), German *Dame*, Russian *Shashki*. The rules engine must be designed so a variant is a config plus a ruleset module.

---

## 8. Non-functional requirements

| ID | Requirement |
|---|---|
| NFR-1 | Runs in current Chrome, Firefox, Safari and Edge on desktop, plus iOS Safari and Android Chrome. **No install, no plugins.** |
| NFR-2 | Responsive layout from 360 px phone (portrait for checkers, landscape recommended for pool) up to desktop. |
| NFR-3 | Authoritative server: it decides turn ownership, rules, physics results and ratings (URD NFR-2). |
| NFR-4 | Gameplay stays fair at up to 1,000 ms latency (URD NFR-4). |
| NFR-5 | First load ≤ 3 s on 4G. Initial download ≤ 800 KB gzip **including the WASM module and Three.js**, excluding audio/art. |
| NFR-6 | Capacity: 1,000 concurrent users on a single server process at MVP. One Rust process can serve far more turn-based connections than that, so scale up the machine before adding nodes. |
| NFR-7 | Availability 99.5 %. Deploys must not cut off running games: the server announces the restart, stops new games, and waits up to 15 min for running ones to finish (§9.7). |
| NFR-8 | Privacy: GDPR-compliant (EU hosting, minimal data, account deletion, **no cookies at all**: the session lives in `localStorage`, strictly necessary storage only, so no consent banner is needed. Google sign-in loads only when the player clicks it). Chat logs kept ≤ 30 days. |
| NFR-9 | Accessibility per §2: WCAG 2.2 AA for non-game UI, scalable text, colour-blind-safe ball and piece styling. |
| NFR-10 | Localisation-ready (i18n strings). Launch languages: English + German. |
| NFR-11 | Security: rate limits on all socket messages, input validation, no client-trusted state, TLS everywhere, signed session tokens, strict Content-Security-Policy, no passwords stored (A-2). |

---

## 9. Platform & technology recommendations

### 9.1 Stack (decided)

| Layer | Choice | Notes |
|---|---|---|
| **Game & client logic** | **Rust**, compiled to **WebAssembly** (`wasm-bindgen`, `wasm-pack`) | **All** logic lives in Rust: rules, physics, bots, rating, the protocol, and the client state machine. The same crates compile natively for the server and to WASM for the browser, so there is a single source of truth |
| **Rendering** | **Three.js** | Game tables only. Orthographic top-down camera + flat materials give the classic 2D look (§6.4). A 3D theme can come later (P-30) |
| **UI shell (lobby, chat, menus, high scores)** | Plain **HTML/CSS + a thin TypeScript layer**, no framework, built with **Vite** | Real DOM text keeps it accessible (screen readers, text scaling, WCAG). TS only renders what the WASM tells it and passes user input back. No game decisions happen in TS |
| **Server** | **Rust + axum** (tokio) | One binary that does three things: delivers the SPA, runs the WebSocket game server, and answers `/status` (§9.4) |
| **Rooms / lobby / tables** | Own implementation: one **tokio task (actor) per lobby and per table**, with messages via channels | Replaces Colyseus. A table actor owns its state, timers and bot. No locks are shared between tables |
| **Pool physics** | Custom 2D ball physics in `boomer-core` | Fixed timestep, analytic ball/cushion collisions, spin. Determinism: `f64` with basic operations only, plus the **`libm` crate** for every trig/sqrt-type function on **both** targets. A CI test checks that native and WASM produce bit-identical results |
| **Checkers engine** | Bitboard move generator in `boomer-core` | Small and fully testable |
| **Computer opponents** | `boomer-core`. Checkers: alpha-beta with a depth per level. Pool: a candidate-shot search in the physics sim with deliberate error per level | Bots run server-side inside the table actor as a virtual seat |
| **Database** | **PostgreSQL** via **sqlx** (compile-time checked queries, migrations) | Schema in §9.5 |
| **Auth** | Email magic link (sent via SMTP with `lettre`, through an EU mail provider) + **Google Identity Services** ID token verified by the server | Everything runs over the WebSocket (§9.6), so no extra HTTP auth routes are needed |
| **Serialisation** | `serde` / `serde_json` | Message types are defined once in Rust and used by both server and WASM |
| **Hosting** | **Google Cloud**: one container on a **Compute Engine VM** + **Cloud SQL for PostgreSQL**, region `europe-west3` (Frankfurt), **Cloudflare** in front | Details and reasons in §9.7 |
| **Ops** | `tracing` (JSON logs → Cloud Logging), Cloud Monitoring uptime check on `/status`, GitHub Actions CI/CD | — |
| **Testing** | `cargo test` + `proptest` (rules, physics invariants), golden replays native vs. WASM, Playwright for end-to-end two-browser matches | — |

**Repository layout**
```
Cargo.toml                 (workspace)
crates/
  boomer-core/             pure logic, no I/O: pool (physics, 8-ball rules), checkers, bots, elo, protocol types
  boomer-client/           wasm-bindgen crate: WebSocket handling, client state machine, input checks,
                           shot animation stepping; exposes a small API + view-model updates to JS
  boomer-server/           axum + tokio + sqlx: SPA delivery, /ws, /status, lobby & table actors, DB
web/                       Vite + TypeScript: index.html, CSS, DOM UI, Three.js scenes, glue to boomer-client
migrations/                sqlx SQL migrations (§9.5)
```

**JS ↔ WASM boundary.** JS calls `client.input(event)` for clicks, aiming, power and chat. The WASM calls back with `onView(viewModel)` for UI and table changes, and exposes `poolFrame(t)` → ball positions for each animation frame. The WASM owns the WebSocket, so JS never sees protocol messages.

### 9.2 Pool synchronisation model
1. The active client sends shot parameters only.
2. The server validates the shot, runs `boomer-core`'s simulation to rest, and applies the 8-ball rules (foul, group, turn).
3. The server broadcasts `{shot params, final ball positions, events (pots/fouls)}`.
4. Every client runs the **identical Rust simulation** (as WASM) for the animation, then **snaps to the server's final state**. With the determinism rules in §9.1 the snap should never be visible. It stays as a safety net.

This keeps bandwidth tiny, the animation smooth, and the server authoritative (URD NFR-3), without having to stream keyframes.

### 9.3 Alternatives not taken
- **TypeScript + Colyseus** (earlier draft) would be faster to start, but the logic would not be Rust/WASM. Replaced by decision D-11.
- **Bevy** compiled to WASM: Rust for rendering too, but large downloads and poor DOM/accessibility for the lobby UI.
- **PixiJS** is a lighter 2D renderer, but Three.js was chosen and keeps a 3D theme open.

### 9.4 Architecture & HTTP surface

```
Browser
  web/ (TS + HTML/CSS)  ── DOM UI: lobby, chat, menus, high scores
     │  view models ▲   ▼ input events
  boomer-client (WASM)  ── all client logic, protocol, sim replay
     │  Three.js ◄── ball/piece positions per frame
     │
     │  WSS  /ws   (JSON text frames, §9.6)
     ▼
boomer-server (axum, single binary)
  ├─ GET /           SPA: index.html, JS, WASM, assets (static, cached; unknown paths → index.html)
  ├─ GET /ws         WebSocket upgrade → connection task
  ├─ GET /status     health & stats JSON
  ├─ Lobby actors    (one per room: roster, chat, table list, invites)
  ├─ Table actors    (one per table: seats, game state, timers, bots)
  └─ sqlx ──► PostgreSQL
```

**Paths:** the server exposes exactly the SPA and `/status`. The WebSocket endpoint `/ws` is how the SPA talks to the server and isn't a separate API. There is no REST API: login, profile and high scores also go over `/ws`.

**`GET /status`**: public, no auth, no personal data. Use it for uptime monitoring and the Docker health check.
```json
{ "status": "ok", "version": "0.1.0", "uptimeS": 86400,
  "db": "ok", "online": 143, "tables": 27, "games": 19 }
```
It returns HTTP 200 when healthy and 503 with `"status": "degraded"` when the DB is unreachable. Responses are cached for a few seconds.

**SPA delivery:**
- Hashed asset file names, cached for 1 year (`immutable`). `index.html` is `no-cache`.
- WASM is served with `application/wasm` so it can be streamed and compiled.
- gzip/brotli via `tower-http`.
- Strict CSP: `script-src 'self' 'wasm-unsafe-eval' https://accounts.google.com`.

### 9.5 Database schema (minimal)

**Rule: the database stores only what must survive a server restart.** Everything live stays in server memory (the lobby and table actors), including:
- rooms, presence, the roster and user status
- tables, seats, spectators and invites
- running game state (a server restart ends running games)
- room and table chat (only a snapshot is stored when someone files a report)
- guests (no row; guest mutes and bans are per session/IP, in memory)
- sound and contrast settings (browser `localStorage`)

**Four tables:**

```sql
-- Registered players. Guests have no row.
CREATE TABLE users (
  id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  email            citext      NOT NULL UNIQUE,          -- magic link + Google both give an email
  google_sub       text        UNIQUE,                   -- NULL if never used Google sign-in
  display_name     citext      NOT NULL UNIQUE,          -- A-3
  name_changed_at  timestamptz,                          -- A-3: once per 30 days
  avatar           smallint    NOT NULL DEFAULT 0,       -- A-4: index into built-in set
  ignored_ids      bigint[]    NOT NULL DEFAULT '{}',    -- L-4: persistent ignore list
  is_moderator     boolean     NOT NULL DEFAULT false,   -- M-2
  muted_until      timestamptz,                          -- M-2
  banned_until     timestamptz,                          -- M-2 (far future = permanent)
  session_version  integer     NOT NULL DEFAULT 0,       -- bump to log out everywhere / on ban
  created_at       timestamptz NOT NULL DEFAULT now()    -- A-5: member since
);

-- One row per (player, game): rating + high score counters.
CREATE TABLE player_stats (
  user_id          bigint   NOT NULL REFERENCES users ON DELETE CASCADE,
  game             text     NOT NULL CHECK (game IN ('pool','checkers')),
  rating           integer  NOT NULL DEFAULT 1200,       -- R-1 (Elo)
  rated_games      integer  NOT NULL DEFAULT 0,          -- R-3: provisional if < 10
  games            integer  NOT NULL DEFAULT 0,          -- H-4: games vs. humans
  wins             integer  NOT NULL DEFAULT 0,          -- H-1: high score count
  wins_reached_at  timestamptz,                          -- H-4: tie-breaker
  PRIMARY KEY (user_id, game)
);
CREATE INDEX player_stats_highscore ON player_stats (game, wins DESC, wins_reached_at);

-- Finished games only (humans and bots). Source for history and anti-farming.
CREATE TABLE matches (
  id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  game                  text     NOT NULL CHECK (game IN ('pool','checkers')),
  player1_id            bigint   REFERENCES users ON DELETE SET NULL,  -- NULL = guest/deleted
  player2_id            bigint   REFERENCES users ON DELETE SET NULL,  -- NULL = guest/deleted/bot
  bot_level             smallint,                         -- E-1: 1..3 if player2 is the computer
  winner                smallint CHECK (winner IN (1,2)), -- NULL = draw
  end_reason            text     NOT NULL,                -- 'win','resign','timeout','disconnect','draw'
  rated                 boolean  NOT NULL,
  counts_for_highscore  boolean  NOT NULL,                -- H-2/H-7 decided at write time
  ended_at              timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX matches_p1 ON matches (player1_id, ended_at DESC);
CREATE INDEX matches_p2 ON matches (player2_id, ended_at DESC);

-- M-1/M-2: user reports with the chat snapshot.
CREATE TABLE reports (
  id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  reporter_id       bigint REFERENCES users ON DELETE SET NULL,   -- NULL = guest
  reported_id       bigint REFERENCES users ON DELETE SET NULL,   -- NULL = guest
  reported_name     text   NOT NULL,                              -- also covers guests
  reason            text   NOT NULL,
  chat_log          text,                                         -- NFR-8: wiped 30 days after resolve
  created_at        timestamptz NOT NULL DEFAULT now(),
  resolved_at       timestamptz,
  resolved_by       bigint REFERENCES users ON DELETE SET NULL
);
```

**How the requirements map onto it:**

| Requirement | Where |
|---|---|
| Profile: rating, W/L/D, last 20 matches (A-5) | `player_stats` + `matches` ordered by `ended_at` |
| High score table, Top 100, "Find me" (H-1, H-5) | `player_stats` via the `player_stats_highscore` index |
| 5-wins-per-day-per-opponent limit (H-7, R-4) | `COUNT(*)` on `matches` for the same pair today `WHERE counts_for_highscore` |
| Computer games: unrated, not on the high score table (E-4, H-2) | `matches.bot_level IS NOT NULL` → `rated = false`, `counts_for_highscore = false` |
| Account deletion (NFR-8) | `DELETE FROM users`: stats cascade, matches and reports keep the row with a NULL user |

**After every finished game, one transaction:** insert `matches`. For each registered human: upsert `player_stats` (rating if rated, `games`, and `wins` + `wins_reached_at` if `counts_for_highscore`).

**Not needed:**
- **No sessions table:** a signed session token carries `user_id` + `session_version`. The SPA keeps it in `localStorage` and sends it in `hello`, and the server checks it against `users`.
- **No login-token table:** magic links (`/#login=<token>`) are signed, expire after 15 minutes, and carry the email.
- **No rooms, tables, chat, friends or settings tables.**

Rating history, tournaments and friends get their own tables in Phase 2.

### 9.6 WebSocket message format

#### Basics
- **Transport:** **one** WebSocket per client at `/ws`, using UTF-8 **JSON text frames** (meets URD NFR-1 literally).
- **Envelope:** every message is `{ "t": "<type>", "d": { …payload } }`. In Rust it's one `enum` per direction with `#[serde(tag = "t", content = "d")]`, defined in `boomer-core::protocol` and shared by server and WASM, so the two sides can't drift apart.
- **One connection** carries lobby and table traffic. A client is in one lobby and at most one table at a time, so table messages need no table ID.
- **Snapshots + events:** the server sends a full **snapshot** on join and reconnect, then small **events**.
- **Handshake:** the first client message must be `hello`. Anything else closes the connection.
- **Limits:** client messages are at most 4 KB. Chat is limited to 1 msg/s (L-3), and every other message to 10 per second. Clients that exceed the limits get errors and are disconnected if they continue. Every 20 s the server sends a ping frame, and the connection is considered dead after 2 missed pongs, which triggers the disconnect grace period (G-7).
- **Conventions:**
  - Message types are `lower.dot.case`. Field names are short camelCase.
  - Durations are milliseconds remaining (`msLeft`), so client clocks don't need to be in sync.
  - IDs are strings: `"u123"` for a registered user, `"g4821"` for a guest, `"bot2"` for the computer at level 2.

#### Connection & account
| Direction | Type | Payload |
|---|---|---|
| C→S | `hello` | `{ v: 1, session?: string, guest?: string }`: protocol version plus the stored session or guest token |
| S→C | `welcome` | `{ me: User, guest?: string, rooms: [{ id, game, name, tier, users }] }`: a new guest gets a token to keep for reconnects. A wrong `v` gets `error upgrade_required` and the client reloads |
| C→S | `auth.email` | `{ email }`: the server sends a magic link. The answer is always `auth.sent`, which doesn't reveal whether the account exists |
| S→C | `auth.sent` | `{}` |
| C→S | `auth.magic` | `{ token }`: from the `/#login=…` link |
| C→S | `auth.google` | `{ idToken }`: from Google Identity Services |
| S→C | `auth.ok` | `{ session, me: User, isNew }`: if `isNew`, the client asks the player to choose a name and avatar |
| C→S | `auth.logout` | `{}` |
| C→S | `account.update` | `{ name?, avatar? }` (A-3, A-4) |
| C→S | `account.delete` | `{}` (NFR-8). The server closes the connection afterwards |
| C→S | `profile.get` | `{ userId }` |
| S→C | `profile` | `{ user: User, since, stats: [{ game, rating, games, wins, losses, draws }], last: [{ game, opp, result, ts }] }` (A-5) |
| C→S | `highscores.get` | `{ game, around?: true }`: Top 100, or the rows around the player's own position (H-5) |
| S→C | `highscores` | `{ game, rows: [{ rank, id, name, avatar, wins, games }] }` |

#### Shared objects
```jsonc
// User: appears in the roster, in seats and in invites
{ "id": "u123", "name": "LordOfTheRails", "avatar": 4, "rating": 1711, "prov": false,
  "status": "available" }        // available | waiting | playing | watching
// Bots: { "id": "bot2", "name": "House Bot (Medium)", "bot": true, ... }

// TableSummary: one row in the lobby table list (T-1)
{ "id": "t29", "no": 29, "seats": [User|null, User|null], "type": "public",   // public | protected | private
  "rated": true, "timer": 30, "opts": { /* game options, see below */ },
  "watchers": 3, "phase": "waiting" }                                          // waiting | playing | over

// Settings: sent by the host when creating a table or changing its settings
{ "type": "public", "rated": true, "timer": 30, "muteSpectators": false, "opts": {} }
// Pool opts:     { "aim": "off|basic|extended", "callEight": false, "bih": "anywhere|behind" }
// Checkers opts: { "clock": "move|game", "gameMin": 10 }   // timer = seconds per move if clock = "move"

// ChatLine: one chat message or system message
{ "from": "u123", "name": "LordOfTheRails", "text": "nice shot", "ts": 1763900000000 }
{ "sys": true, "text": "LordOfTheRails's old rating: 1722; new rating: 1738", "ts": … }
```

#### Lobby
| Direction | Type | Payload |
|---|---|---|
| C→S | `lobby.join` | `{ room }`: enter a room (switching rooms leaves the old one) |
| S→C | `lobby.snapshot` | `{ room, me: User, users: User[], tables: TableSummary[], chat: ChatLine[] (last 50), online }` |
| S→C | `user.add` / `user.update` / `user.remove` | `User` / `User` / `{ id }` |
| S→C | `table.upsert` / `table.remove` | `TableSummary` / `{ id }` |
| S→C | `chat` | `ChatLine` |
| S→C | `invite.received` | `{ inviteId, from: User, game, settings, msLeft: 30000 }` |
| S→C | `invite.closed` | `{ inviteId, reason: "declined"\|"expired"\|"cancelled" }` |
| C→S | `chat.send` | `{ text }` (≤ 300 chars) |
| C→S | `table.create` | `Settings` |
| C→S | `play.now` | `{ vsBot?: 1\|2\|3 }`: without `vsBot` it looks for a human first (E-3) |
| C→S | `invite.send` | `{ to, tableId? , settings? }`: either an existing table or new settings |
| C→S | `invite.answer` | `{ inviteId, accept }` |
| C→S | `ignore.set` | `{ userId, on }`: saved to the account (L-4). The client does the hiding |
| C→S | `report` | `{ userId, reason }`: the server attaches its in-memory chat log |

#### Table
| Direction | Type | Payload |
|---|---|---|
| C→S | `table.join` / `table.leave` | `{ tableId, as: "player"\|"spectator" }` / `{}` |
| S→C | `table.snapshot` | `{ id, no, host, settings, seats: [User\|null, User\|null], ready: [bool,bool], watchers: User[], phase, chat: ChatLine[], game: GameState\|null, turn, activeSeat, msLeft, clocks? }`. Sent on `table.join`, after Create, Play Now or an accepted invite (the server seats the player directly), **and on reconnect** (FR-8.2) |
| S→C | `table.update` | Any changed subset of `{ settings, seats, ready, watchers, host, phase }` |
| S→C | `chat` | `ChatLine` (table chat, including system lines) |
| S→C | `game.start` | `{ game: GameState, firstSeat }` |
| S→C | `turn.start` | `{ turn, seat, msLeft, clocks?: [ms, ms], bih?: true }` |
| S→C | `game.action` | `{ turn, seat, action, result }`: the verified move, broadcast to everyone (FR-7.4) |
| S→C | `game.pause` / `game.resume` | `{ seat, msLeft: 45000 }` / `{}` |
| S→C | `draw.offered` | `{ seat }` (checkers) |
| S→C | `game.over` | `{ winner: 1\|2\|null, reason, rating?: [{ id, old, new }], wins?: [{ id, wins }] }` |
| S→C | `rematch.state` | `{ ready: [bool, bool] }` |
| S→C | `kicked` | `{ reason: "booted"\|"table_closed" }`: the client goes back to the lobby |
| C→S | `chat.send` | `{ text }` |
| C→S | `ready` | `{}`: Start / Ready (G-4) |
| C→S | `settings.set` | `Partial<Settings>`: host only, `waiting` phase only |
| C→S | `seat.take` / `seat.leave` | `{ seat: 1\|2 }` / `{}`: Stand Up (T-7); leaving mid-game = resign |
| C→S | `bot.add` / `bot.remove` | `{ level: 1\|2\|3 }` / `{}`: host fills the empty seat with the computer (E-1) |
| C→S | `boot` | `{ userId }`: host only (T-6) |
| C→S | `action` | `{ turn, ...gameAction }`: see below. `turn` must match, otherwise `error stale_turn` |
| C→S | `resign` | `{}` |
| C→S | `draw.offer` / `draw.answer` | `{}` / `{ accept }` |
| C→S | `rematch` | `{}` |

#### Game payloads: Pool
Coordinates are in **metres** with the table origin at the bottom-left of the playing surface (2.54 × 1.27 m). The server sends exact JS numbers, so clients start every replay from identical positions.
```jsonc
// GameState
{ "balls": [[0, 0.635, 0.635], [1, 1.905, 0.635], …],   // [number, x, y]; pocketed balls are omitted
  "groups": [null, null],                               // per seat: "solids" | "stripes" | null (open table)
  "pocketed": [[], []] }                                // ball numbers per seat (P-15)

// action (C→S)
{ "turn": 17, "angle": 1.5708, "power": 0.72, "spinX": 0, "spinY": -0.3,
  "cue": [0.5, 0.6] }        // only with ball-in-hand

// result (inside game.action, S→C)
{ "balls": [[0, 1.2, 0.4], …],                           // final resting positions → clients snap to these
  "events": [ { "t": "pot", "ball": 3, "pocket": 2 },
              { "t": "foul", "kind": "scratch" } ],       // scratch|wrong_first|no_hit|no_rail|off_table
  "groups": ["solids", "stripes"] }
```
The client animates by running the shared simulation on the `action`, then snaps to `result.balls` (§9.2). The next shooter and ball-in-hand arrive in `turn.start`. When the timer runs out, the server sends `turn.start` with `bih: true` for the opponent and no `game.action` (D-9).

#### Game payloads: Checkers
Squares use standard notation **1–32** (Black at 1–12, which moves first).
```jsonc
// GameState
{ "board": "bbbbbbbbbbbb........wwwwwwwwwwww",   // 32 chars: b w = men, B W = kings, . = empty
  "moves": ["11-15", "23-19"] }                 // move list (C-10)

// action (C→S)
{ "turn": 8, "path": [22, 15, 8] }               // simple move = 2 squares; a jump lists every landing square

// result
{ "captured": [18, 11], "crowned": false, "board": "…" }
```

#### Errors (S→C)
`error { code, re }`, where `re` is the message type that failed. Codes:
- `not_your_turn`, `stale_turn`, `illegal_action`
- `not_host`, `table_full`, `not_allowed`
- `muted`, `rate_limited`, `invalid`
- `upgrade_required`, `banned`

The client shows a short, friendly text for each code. Messages from the waiting player or from spectators are rejected with `not_your_turn` (FR-7.3).

#### Typical flows
1. **Start:** open `/ws` → `hello` → `welcome` → `lobby.join` → `lobby.snapshot`.
2. **Play Now:** `play.now` → `table.snapshot` → both send `ready` → `game.start` → `turn.start`.
3. **One shot:** `action` → the server simulates → `game.action` to everyone → animation → `turn.start` for the next player.
4. **Login:** `auth.email` → `auth.sent` → the player clicks the link → the SPA opens `/#login=…` → `auth.magic` → `auth.ok` (the SPA stores `session`).
5. **Reconnect:** the connection drops → others get `game.pause` → the player reconnects within 45 s → `hello { session }` → `welcome` + `lobby.snapshot` + `table.snapshot` → `game.resume`. After 45 s → `game.over { reason: "disconnect" }`.

### 9.7 Deployment on Google Cloud

**Key constraint:** all live state (lobbies, tables, running games) lives in the memory of **one** server process (§9.5). So there must be exactly **one** instance at a time, and WebSocket connections must be able to stay open for hours.

**Recommendation: one Compute Engine VM running the container.**

| Component | Choice | Approx. cost/month |
|---|---|---|
| VM | **e2-small** (2 shared vCPU, 2 GB) in `europe-west3`, Container-Optimized OS, running the one container. Upgrade to e2-medium if `/status` shows load | ~ $15 |
| Database | **Cloud SQL for PostgreSQL**, smallest shared-core tier, private IP only, automatic daily backups + point-in-time recovery | ~ $10–15 |
| Static IP | 1 external IPv4 | ~ $3 |
| Images | **Artifact Registry** (`europe-west3`) | < $1 |
| Secrets | **Secret Manager**: session signing key, SMTP password, Google client ID, DB password | < $1 |
| Logs / monitoring | Cloud Logging + a Cloud Monitoring uptime check on `/status` with email alerts | free tier |
| TLS / edge | **Cloudflare** (free plan) proxies the domain. axum serves TLS itself with a Cloudflare Origin Certificate (rustls). This avoids Google's HTTPS load balancer (~$18+/month) | $0 |
| **Total** | | **≈ $30–35** |

Boomergames makes no money, so the owner pays this.

**Container:**
- Multi-stage Dockerfile: build the Rust server and the web/WASM bundle, then copy both into a **distroless** runtime image (≈ 30 MB).
- One process. Config comes from environment variables and Secret Manager. The container runs as non-root.

**Network & security:**
- The VM firewall allows port 443 **only from Cloudflare's IP ranges**. SSH only through **IAP** (no public SSH port).
- Cloud SQL is reachable only over private IP from the VM.
- A dedicated service account gets only the roles it needs: Secret Manager read, Artifact Registry read, Cloud SQL client and log writing.

**Deploys (GitHub Actions → Artifact Registry → VM):**
1. CI builds and tests, then pushes the image. Authentication uses **Workload Identity Federation**, so there are no stored keys.
2. The deploy step tells the running server to **drain**:
   - it broadcasts a system chat line ("Server update in a few minutes — running games will finish")
   - it refuses new games
   - it waits until running games end (max 15 min)
3. Pull the new image and restart the container. Downtime is a few seconds, and clients reconnect automatically.
4. Run DB migrations at startup (`sqlx migrate`).

**Email:** Google Cloud blocks outgoing port 25. Send magic links over **port 587** through an EU mail provider (e.g. Brevo or Mailjet) with SPF/DKIM set up for the domain.

**Why not Cloud Run?** It's the usual "container on Google Cloud" answer, but it fits this design poorly:
- **WebSockets are cut after at most 60 minutes.** That's tolerable on its own, because clients reconnect.
- **Instances can be replaced or duplicated at any time**, and during a deploy old and new revisions run **at the same time**. With in-memory lobbies, that splits players across two servers and loses running games.
- Making it work would require "max 1 instance, CPU always allocated", and it still wouldn't fix the deploy split. That costs about the same as the VM and is more fragile.

Cloud Run becomes interesting only if the live state ever moves out of memory into an external store (e.g. Memorystore/Redis), which is not planned.

**GKE (Kubernetes)** is unnecessary for one container.

---

## 10. Delivery phases

| Phase | Content | Exit criterion |
|---|---|---|
| **0 — Foundations** (2 wks) | Cargo workspace + Vite/wasm-pack build, axum serving SPA + `/status` + `/ws`, sqlx migrations, auth over WS, Docker deploy, CI | Two browsers connect, log in and see each other |
| **1a — Lounge** (4 wks) | Lobby and table actors (own implementation, no Colyseus), roster, chat, table list, create/join/watch, invites, Play Now, ignore, reconnect | Full lobby flow works with dummy "games" |
| **1b — Checkers** (4 wks) | Rules engine + tests, board UI, clocks, draw/resign, ratings, rematch, **checkers AI (3 levels)**, house tables | Rated checkers match end-to-end, spectators included; bot always available |
| **1c — Pool** (7 wks) | Deterministic physics + golden tests, 8-ball rules, aiming/power/spin UI, ball-in-hand, **classic visual style (§6.4)**, **pool bot (3 levels)** | 1,000 random shots are bit-identical native vs. WASM and in 3 browsers; Medium bot beats a beginner |
| **1d — Polish & launch** (3 wks) | Disconnect handling, moderation, accessibility pass, sounds, mobile tuning, load test | 200-user load test passes. Closed beta: the first players are **personally invited by the project owner** (invite links) |
| **2** | Friends, tournaments, leaderboards, 9-ball, draughts variants, modern theme, PWA | — |

Checkers goes first on purpose: it exercises the whole platform (lobby, turns, timers, ratings, reconnects) with a small rules engine. Pool then adds only the physics risk on top of that.

---

## 11. Success metrics (beta)
- Time from landing to first move: **< 60 s** (guest)
- ≥ 40 % of beta users return in week 2
- Average session ≥ 20 min. ≥ 30 % of sessions include chat.
- Zero desync reports in pool. < 1 % of games end in disconnect-forfeit.

---

## 12. Decisions

| # | Topic | Decision |
|---|---|---|
| D-1 | Name | **Boomergames** is the public product name |
| D-2 | Checkers ruleset | **English draughts / American checkers** for the MVP |
| D-3 | Empty rooms | Computer opponents and house tables are **in the MVP** (§5.6) |
| D-4 | Pool look | **Classic 2000s style** (§6.4), with our own art |
| D-5 | Guests | Guests may **chat** (A-1a) |
| D-6 | Aiming aid | The **table host** chooses the level, rated games included (P-11) |
| D-7 | High score table | **Wins against humans only**, per game (§5.7) |
| D-8 | Business model | **None.** Completely free, no money involved (§3.3) |
| D-9 | Pool timer expiry | **Ball-in-hand** to the opponent (G-6) |
| D-10 | First players | **Personally invited by the project owner** (closed beta via invite links) |
| D-11 | Tech stack | **Three.js** client, **all logic in a Rust → WASM package**, **Rust axum** server that delivers the SPA and `/status` (§9) |
| D-12 | Hosting | **Google Cloud**: Compute Engine VM with the container + Cloud SQL, `europe-west3`, Cloudflare in front (§9.7) |

## 13. Pending (owner to-dos, not blocking development)
1. **Domain:** register a domain for Boomergames (e.g. boomergames.com / .de).
2. **Trademark:** check the name "Boomergames" before public launch.

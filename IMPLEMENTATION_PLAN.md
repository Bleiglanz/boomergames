# Boomergames — Implementation Plan

*Engineering roadmap derived from [incubation/plan.md](./incubation/plan.md) and [incubation/turn_based_multiplayer_game_platform_requirements.md](./incubation/turn_based_multiplayer_game_platform_requirements.md).*

## 1. Goals & Guiding Principles

Build **Boomergames**—a completely free, browser-based classic games platform starting with **Checkers (English Draughts)** and **8-Ball Pool**—guided by four engineering pillars:

1. **Rust Everywhere (Single Source of Truth):** Game rules, deterministic physics, AI bots, Elo ratings, protocol definitions, and client state machine all live in Rust. Native (`x86_64`) and browser (`wasm32-unknown-unknown`) run the exact same code.
2. **Strict `clippy::pedantic` Everywhere:** Workspace-wide lint enforcement (`clippy::pedantic = "deny"`, `unsafe_code = "forbid"`, and no `unwrap`/`expect`/`panic`/`indexing_slicing` in production code), checked on both native and WASM targets with `-D warnings`.
3. **Clean Architecture (Ports & Adapters):** Dependencies point strictly inward toward `boomer-domain`. A `GameTransport` port trait decouples the client state machine from delivery (`LocalSessionAdapter` in Step 1, `WebSocketAdapter` in Step 2).
4. **Two-Step Delivery:**
   - **Step 1 (Milestones 0–3):** Offline HTML/WASM single-player client (Hotseat + 3-level Computer AI) for Checkers and 8-Ball Pool—**no server required**.
   - **Step 2 (Milestones 4–7):** Authoritative single-binary Rust (`axum` + `tokio`) backend with actor-based lobbies/tables, PostgreSQL (`sqlx`), passwordless auth, ratings, high scores, moderation, and GCP deployment.

---

## 2. Architecture Overview

### 2.1 Clean Architecture Dependency Graph

```mermaid
flowchart TD
    subgraph Inner["Inner Ring (Zero I/O, Pure Rust)"]
        DOM["boomer-domain\n(Checkers, Pool Physics & 8-Ball Rules,\nAI Bots, Elo, Domain Ports)"]
        PROTO["boomer-protocol\n(serde JSON Envelope, DTOs,\nError Codes, Commands/Events)"]
    end

    subgraph Client["Step 1 & 2 Browser Adapter"]
        WASM["boomer-client (WASM)\n(App State Machine, GameTransport Port,\nLocalSessionAdapter, WebSocketAdapter,\nAnimation Sampler)"]
        WEB["web/ (Vite + TS + Three.js)\n(DOM Chrome, Orthographic 2D Scenes,\nAudio, Input Forwarding)"]
    end

    subgraph Server["Step 2 Authoritative Backend"]
        SRV["boomer-server (axum + tokio)\n(HTTP/SPA, /ws, /status,\nLobby & Table Actors, sqlx Postgres,\nSMTP/Google Auth)"]
    end

    PROTO --> DOM
    WASM --> DOM
    WASM --> PROTO
    WEB --> WASM
    SRV --> DOM
    SRV --> PROTO
```

### 2.2 Step 1 → Step 2 Transport Seam

```mermaid
flowchart LR
    UI["web/ (TS + Three.js)"] -- "UiInput / ViewModel" --> APP["boomer-client\n(ClientApp)"]
    APP -- "ClientMessage / ServerMessage" --> PORT["GameTransport Trait"]
    PORT -- "Step 1 (Offline)" --> LOCAL["LocalSessionAdapter\n(Runs boomer-domain +\nHotseat / AI locally)"]
    PORT -- "Step 2 (Online)" --> WS["WebSocketAdapter\n(WSS /ws to boomer-server)"]
    WS --> SRV["boomer-server\n(TableActor + boomer-domain)"]
```

### 2.3 Target Repository Layout

```text
Boomergames/
├── Cargo.toml                     # Workspace root + [workspace.lints]
├── rust-toolchain.toml            # Pinned stable Rust + wasm32-unknown-unknown + clippy/rustfmt
├── justfile                       # Single-command lint, test, wasm build, determinism check
├── IMPLEMENTATION_PLAN.md         # This document
├── crates/
│   ├── boomer-domain/             # Pure domain logic, physics, rules, AI, Elo, domain ports
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── math.rs            # Deterministic f64 + libm wrappers (Vec2, trig, sqrt)
│   │       ├── checkers/          # Bitboard engine, rules, notation, 3-level alpha-beta AI
│   │       ├── pool/              # Physics sim, 8-ball rules, aim raycaster, 3-level shot AI
│   │       └── rating.rs          # Elo calculator & provisional/anti-farming rules
│   ├── boomer-protocol/           # Shared serde wire protocol (C->S and S->C enums + DTOs)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── envelope.rs        # #[serde(tag = "t", content = "d")]
│   │       ├── checkers.rs
│   │       ├── pool.rs
│   │       └── lobby.rs           # Table/Lobby/Auth/Profile/Highscore messages
│   ├── boomer-client/             # WASM crate: client state machine + transport adapters
│   │   └── src/
│   │       ├── lib.rs             # Thin wasm-bindgen API surface
│   │       ├── app.rs             # Client state machine -> ViewModel projection
│   │       ├── ports.rs           # GameTransport, ClockPort, RandomPort traits
│   │       ├── local_adapter.rs   # Step 1 offline single-player (Hotseat + Bot runner)
│   │       ├── ws_adapter.rs      # Step 2 WebSocket adapter
│   │       └── animation.rs       # Pool shot timeline buffer + pool_frame(t_ms)
│   └── boomer-server/             # Step 2: axum binary (actors, sqlx, auth, SPA, /status)
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── ports/             # UserRepo, MatchRepo, MailSender, TokenVerifier traits
│           ├── actors/            # LobbyActor, TableActor, BotWorkerPool
│           └── adapters/          # axum HTTP/WS, sqlx Postgres, lettre SMTP, Google OIDC
├── migrations/                    # Step 2: sqlx PostgreSQL migrations (4-table schema)
└── web/                           # Vite + TypeScript + Three.js UI shell
    ├── package.json
    ├── vite.config.ts
    ├── index.html
    └── src/
        ├── main.ts
        ├── wasm_bridge.ts         # Thin wrapper calling boomer-client WASM
        ├── i18n/                  # EN + DE string tables
        ├── audio/                 # Procedural/license-free era-appropriate sound effects
        ├── ui/                    # Accessible DOM chrome (header, menus, controls, HUD)
        └── render/
            ├── renderer.ts        # Scene interface
            ├── checkers_scene.ts  # Three.js orthographic 2D checkers board
            └── pool_scene.ts      # Three.js orthographic 2D classic pool table
```

---

## 3. Workspace Lint & Quality Policy

Every crate inherits `[workspace.lints]` via `[lints] workspace = true`.

```toml
[workspace]
resolver = "3"
members = [
    "crates/boomer-domain",
    "crates/boomer-protocol",
    "crates/boomer-client",
    # "crates/boomer-server", # Added in Step 2 (Milestone 4)
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.88"

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unreachable_pub = "warn"

[workspace.lints.clippy]
pedantic = { level = "deny", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
indexing_slicing = "deny"
```

- **Test Relaxation:** Crate roots opt into test ergonomics via `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing))]`.
- **Justified Exceptions:** Any rare exception (e.g., `wasm-bindgen` bindings or deliberate numeric casts in physics) must use `#[expect(clippy::..., reason = "...")]` so stale suppressions fail compilation.

---

## 4. Step 1 — Offline HTML/WASM Single-Player (No Server Required)

### Milestone 0 — Workspace Foundation, Strict Lints & WASM Vertical Slice

**Goal:** Stand up a zero-warning Rust workspace and Vite/TypeScript/Three.js frontend wired to `boomer-client` WASM before implementing game rules.

- [ ] **0.1 Root Scaffolding:** Create `Cargo.toml` with `[workspace.lints]`, `rust-toolchain.toml` (stable + `wasm32-unknown-unknown` + `clippy` + `rustfmt`), `.gitignore`, and `justfile` (`check`, `test`, `wasm`, `dev`, `determinism`).
- [ ] **0.2 Domain & Protocol Skeletons:**
  - `crates/boomer-domain`: Core types (`GameKind`, `Seat`, `BotLevel`, `GameMode`) and `math.rs` (`libm`-backed deterministic `f64` math helpers).
  - `crates/boomer-protocol`: Versioned JSON envelope (`#[serde(tag = "t", content = "d")]`), `ClientMessage`, `ServerMessage`, `ErrorCode`.
- [ ] **0.3 Client Port & WASM Bridge:**
  - `crates/boomer-client/src/ports.rs`: Define `GameTransport` (`send`, `poll_messages`, `tick`), `ClockPort`, and `RandomPort`.
  - `crates/boomer-client/src/lib.rs`: Expose thin `#[wasm_bindgen]` `BoomerClient` (`new`, `dispatch_input`, `tick`, `view_model`, `pool_frame`).
- [ ] **0.4 Vite + TypeScript + Three.js Shell:**
  - Strict TypeScript config, Three.js orthographic 2D canvas setup, classic 2000s Boomergames CSS theme (cream header, teal-grey side panel, Verdana/Tahoma stack, ≥16px base font, high-contrast CSS variables).
  - Interactive game-picker home screen (Checkers / 8-Ball Pool × Hotseat / vs. Easy / Medium / Hard) driven entirely by WASM `ViewModel`.

**Exit Criteria:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes cleanly.
- `cargo clippy --target wasm32-unknown-unknown -p boomer-domain -p boomer-protocol -p boomer-client -- -D warnings` passes cleanly.
- `wasm-pack build crates/boomer-client --target web` + `cd web && npm run build` produce a working standalone static bundle.

---

### Milestone 1 — Checkers (English Draughts) End-to-End

**Goal:** Deliver a complete, rule-accurate playable Checkers game (Hotseat + 3-level AI) across the full Clean Architecture stack.

- [ ] **1.1 Bitboard Engine & Rules (`boomer-domain::checkers`):**
  - 32-square bitboard state (`black_men`, `white_men`, `black_kings`, `white_kings` as `u32`), mapping standard notation `1..=32` (Black on `1..=12` moves first).
  - Diagonal 1-square moves for men (forward) and kings (forward/backward; no flying kings).
  - Mandatory captures and complete recursive multi-jump generation (player chooses capture path; no max-capture rule).
  - Crowning on far row; mid-jump crowning immediately ends the turn.
  - Win detection (no opponent pieces / no legal moves, resign, timeout) and draw detection (Offer Draw, threefold repetition, 40-move rule without capture or man move).
  - Standard algebraic notation (`11-15`, `22x15`, `22x15x8`).
  - Perft tests, edge-case unit tests, and `proptest` invariants.
- [ ] **1.2 Computer Opponent (`boomer-domain::checkers::ai`):**
  - Deterministic alpha-beta minimax with capture quiescence, move ordering, and positional evaluation.
  - Seeded deterministic PRNG (`SplitMix64`): `Easy` (depth 2 + controlled error), `Medium` (depth 4 + slight jitter), `Hard` (depth 7 + full evaluation).
- [ ] **1.3 Protocol & Offline Client (`boomer-protocol`, `boomer-client`):**
  - `CheckersStateDto`, `CheckersActionDto { path }`, `CheckersResultDto { captured, crowned, notation, board }`, `CheckersSettingsDto` (per-move 20/30/60s or per-game 5/10/15m clock).
  - `LocalSessionAdapter` implementing `GameTransport` for Hotseat and vs. Bot (with natural thinking delay via `tick(dt_ms)`).
  - Multi-jump step-by-step OR single-click unambiguous destination selection, mandatory-capture filtering, clocks, Offer Draw, Resign, and alternating-color Rematch.
- [ ] **1.4 Classic 2D Board & HUD (`web/`):**
  - Three.js orthographic 2D 8×8 wooden/classic board, red & black flat discs with crisp king crowns, legal destination highlights, mandatory-capture highlight + *"You must capture"* banner, last-move highlight.
  - Click-to-move and drag-and-drop input.
  - HUD: player panels, turn indicator, clock, captured-piece count, scrollable notation list, **Flip Board** toggle, Resign / Offer Draw / Rematch controls.

**Exit Criteria:**
- Full Checkers matches playable in Hotseat and vs. Easy/Medium/Hard AI in-browser with zero server.
- All unit tests, `proptest` invariants, and native + WASM `clippy::pedantic` gates pass.

---

### Milestone 2 — 8-Ball Pool End-to-End (Deterministic Physics + Rules + AI + Classic 2D UI)

**Goal:** Deliver a complete, bit-deterministic 8-Ball Pool game with authentic 2000s visual feel, full English spin physics, aiming aids, ball-in-hand, and 3-level AI.

- [ ] **2.1 Deterministic 2D Physics (`boomer-domain::pool::physics`):**
  - Regulation `2.54 × 1.27 m` playing surface (`ball_radius = 0.028575 m`, 6 pockets with rounded jaws, head string at `x = 0.635 m`, foot spot at `x = 1.905 m`).
  - Strictly `f64` + `libm` (`libm::sqrt`, `libm::sin`, `libm::cos`, `libm::atan2`, `libm::hypot`, `libm::fabs`)—zero `std` transcendental intrinsics.
  - Fixed sub-step (`dt = 1/240 s`) with swept ball-ball and ball-cushion collision detection (tunneling-free on hard breaks).
  - Sliding-to-rolling friction transition, top/back spin (`spin_y`: follow/draw), side English (`spin_x`: cushion throw/rebound angle), cushion energy loss, pocket jaw deflection, drop detection, resting threshold, and deterministic step cap.
  - Emits `ShotSimulation` with discrete events (`FirstContact`, `BallHit`, `CushionHit`, `Pot`, `Scratch`, `OffTable`), final resting coordinates, and `1/60 s` keyframes for smooth animation.
- [ ] **2.2 Standard 8-Ball Rules & Aiming Aid (`boomer-domain::pool::{rules, aim}`):**
  - 15-ball triangle rack (8-ball center, solid & stripe in rear corners, seeded shuffle).
  - Break rules (configurable 8-on-break re-rack/win; break scratch → ball-in-hand).
  - Open table until legal post-break pot assigns Solids (`1..=7`) / Stripes (`9..=15`); turn continuation on legal own-group pot.
  - Complete foul matrix (scratch, wrong first contact, no contact, no rail after contact without pot, off-table) → opponent **Ball-in-Hand** (anywhere vs. behind head string, configurable) with placement overlap validator.
  - Optional call-pocket for 8-ball; win/loss resolution (early 8-ball / foul-on-8 / off-table loss).
  - Pure Rust aiming raycaster (`Off`, `Basic` ghost-ball + cue line, `Extended` object-ball + tangent vectors).
- [ ] **2.3 Pool AI (`boomer-domain::pool::ai`):**
  - Candidate shot search over legal target balls and pockets using `physics.rs` (plus safety fallback when no clean pot exists).
  - Seeded deterministic angular/power/spin error scaled across `Easy`, `Medium`, and `Hard`.
- [ ] **2.4 Cross-Target Determinism Gate (`tests/pool_determinism.rs`):**
  - 1,000 seeded shots (breaks, heavy follow/draw, side English, banks, jaw rattles) hashed by exact `(id, x.to_bits(), y.to_bits())` and rule events.
  - Verified bit-identical between native (`cargo test`) and WASM (`wasm-pack test --node`).
- [ ] **2.5 Protocol, Animation Sampler & Local Adapter (`boomer-protocol`, `boomer-client`):**
  - Protocol DTOs matching `plan.md` §9.6 (`PoolStateDto`, `PoolActionDto`, `PoolResultDto`, `PoolSettingsDto`).
  - `animation.rs`: Interpolated `pool_frame(t_ms)` playback + sound event triggers + final snap to `result.balls`.
  - Shot clock expiry passes turn with opponent **Ball-in-Hand** (no match forfeit per `plan.md` D-9).
- [ ] **2.6 Classic 2000s 2D Table & Controls (`web/`):**
  - Three.js orthographic 2D table: `#2E8B2E` cloth, `#8B1A1A`/`#6B1010` maroon rails with tan diamonds & pocket caps, 6 black pockets, faint head string, flat numbered solids/stripes (color-blind distinguishable), light-wood cue stick, aiming aid lines.
  - Controls: desktop mouse aim, touch drag + fine-aim slider, right-side vertical power meter with tick marks, left-side cue-ball English spin widget, drag-to-place ball-in-hand with red/green validity indicator, pocketed balls display, top shot clock.

**Exit Criteria:**
- Full 8-Ball Pool playable in Hotseat and vs. Easy/Medium/Hard AI in-browser with zero server.
- 1,000-shot golden determinism test produces bit-identical output on native and WASM.
- Smooth 60 fps animation on desktop and mobile.

---

### Milestone 3 — Step 1 Polish, Audio, Accessibility & Static Delivery

**Goal:** Complete standalone offline quality, i18n, accessibility, and bundle-size budgets.

- [ ] **3.1 Era-Appropriate Audio (`web/src/audio/`):** Lightweight Web Audio API effects (cue hit, ball click, cushion thud, pocket drop, checker slide/capture/king chime, 5s shot-clock warning) + mute toggle saved in `localStorage`.
- [ ] **3.2 Internationalization (`web/src/i18n/`):** Complete English (`en`) and German (`de`) string tables with runtime switcher.
- [ ] **3.3 Accessibility & Responsive Pass:** WCAG 2.2 AA contrast, ≥16px scalable base font, High-Contrast mode toggle, full keyboard menu & Checkers board navigation, responsive layout down to `360px` width, zero cookies (`localStorage` only).
- [ ] **3.4 Bundle Budget Audit:** Optimize WASM (`wasm-opt -Oz`) and Vite chunks; confirm initial gzipped payload (HTML/CSS/JS + Three.js + WASM) is `≤ 800 KB`.

**Exit Criteria:**
- Static `web/dist/` build serves cleanly with zero backend, passes all lint/test/determinism/bundle gates, and meets WCAG 2.2 AA + EN/DE requirements.

---

## 5. Step 2 — Authoritative Server Backend & Social Lounge

> Step 2 adds `crates/boomer-server` and `migrations/`. `boomer-domain`, `boomer-protocol`, and the Three.js renderers require zero architectural rewrites—`boomer-client` simply plugs in `WebSocketAdapter` behind `GameTransport`.

### Milestone 4 — `boomer-server` Core, SPA Serving, `/status`, PostgreSQL & Auth

- [ ] **4.1 Workspace & Server Skeleton:** Add `crates/boomer-server` to workspace; define server ports (`UserRepo`, `StatsRepo`, `MatchRepo`, `ReportRepo`, `MailSender`, `GoogleTokenVerifier`).
- [ ] **4.2 HTTP Surface (`axum` + `tower-http`):**
  - `GET /` (+ SPA fallback): Immutable 1-year cached hashed assets, `no-cache` `index.html`, `application/wasm`, gzip/brotli, strict CSP (`script-src 'self' 'wasm-unsafe-eval' https://accounts.google.com`).
  - `GET /status`: Cached JSON health endpoint (`status`, `version`, `uptimeS`, `db`, `online`, `tables`, `games`) returning 200 healthy / 503 degraded.
  - `GET /ws`: WebSocket upgrade enforcing `hello` handshake, 4 KB max message size, rate limits (1 msg/s chat, 10 msg/s other), and 20s ping/pong liveness.
- [ ] **4.3 PostgreSQL Schema (`migrations/0001_initial.sql` & `sqlx`):**
  - Implement exact 4-table schema from `plan.md` §9.5 (`users`, `player_stats`, `matches`, `reports`) with compile-time checked queries.
- [ ] **4.4 Passwordless Auth & Client WS Adapter:**
  - Guests (`Guest_XXXX`) with reconnectable guest token (in-memory only, no DB row).
  - Stateless signed session tokens (`user_id` + `session_version`) stored in `localStorage`.
  - Email magic links (`/#login=<token>`, 15-min TTL) via `lettre` over SMTP port 587.
  - Google Identity Services ID token verification (`auth.google`).
  - Unique profanity-filtered display name (30-day change cooldown), avatar picker, GDPR `account.delete`.
  - Implement `crates/boomer-client/src/ws_adapter.rs` (`GameTransport` over `web-sys` WebSocket).

**Exit Criteria:** Two browsers connect to `boomer-server`, authenticate (guest, magic link, or Google), and stay synced over `/ws`.

---

### Milestone 5 — Actor-Based Lounges, Matchmaking, Tables & Chat

- [ ] **5.1 `LobbyActor` (one `tokio` task per room):**
  - 1–2 initial rooms per game, auto-opening new rooms past ~70% of 100-user capacity.
  - Live User Roster (`Available`, `At Table – Waiting`, `In Game`, `Spectating`), room chat (last 50 lines in memory, 300-char limit, 1 msg/s rate limit, profanity filter), persistent Ignore/Mute list.
  - Table list CRUD (`Public`, `Protected`, `Private`), 30s-expiring Direct Invites, `Play Now` (±200 Elo widening window + 20s human-first wait before bot suggestion).
- [ ] **5.2 `TableActor` (one `tokio` task per table, zero shared locks):**
  - Seat 1 (Host), Seat 2, Spectators, isolated Table Chat, Mute Spectators toggle.
  - Host controls (`Boot` with 5-min block, `Invite`, pre-game settings), `Stand Up`, and shareable table links (`/#table=<id>`).
- [ ] **5.3 Lounge & Table UI (`web/src/ui/lobby.ts`):**
  - Classic room selector, live roster, chatbox, table list (`Join`/`Watch`/`Full`), Create Table modal, invite toasts, Ignore/Mute controls.

**Exit Criteria:** Full multi-user lobby, chat, table creation, invites, and spectating flow works end-to-end.

---

### Milestone 6 — Authoritative Multiplayer Games, House Bots, Elo & High Scores

- [ ] **6.1 Authoritative Turn Loop (`TableActor` + `boomer-domain`):**
  - Match lifecycle (`WAITING → READY → IN_PROGRESS → GAME_OVER → Rematch|WAITING`).
  - Strict turn validation (rejects out-of-turn, stale-turn, or spectator moves).
  - Pool authoritative sim on server → broadcast `{ action, result }` → clients replay identical WASM sim and snap to `result.balls`.
- [ ] **6.2 House Bots & On-Demand Bots:**
  - Always-open 🤖 House Bot tables (2–3 per room) and host `bot.add`/`bot.remove`.
  - Bot AI runs on a bounded `spawn_blocking` worker pool so actor tasks never stall.
- [ ] **6.3 Ratings & High Scores (`boomer-domain::rating` + `sqlx`):**
  - Per-game Elo starting at `1200`, provisional `?` under 10 rated games, tier badges, anti-farming rating decay.
  - Win-count High Score table (human wins only, max 5 wins/day/opponent, min move/shot threshold, tie-broken by `wins_reached_at`, Top 100 + *"Find me"*).
  - Single atomic DB transaction per finished match recording `matches` and upserting `player_stats`; profile shows W/L/D and last 20 matches.

**Exit Criteria:** Rated Checkers and 8-Ball Pool matches with spectators, House Bots, live rating updates, and High Score table work end-to-end with zero Pool desyncs.

---

### Milestone 7 — Resilience, Moderation, Load Testing & GCP Production Deploy

- [ ] **7.1 Disconnect & Reconnect:** 45s `PAUSED_DISCONNECT` grace period on socket drop or 2 missed pongs; full `table.snapshot` on reconnect; forfeit + rating update on expiry; repeat-abandonment cooldown.
- [ ] **7.2 Moderation Tools:** User `report` capturing in-memory chat snapshot into `reports` (wiped 30 days after resolution); moderator `mute`, `ban` (with `session_version` bump), and `close_table`.
- [ ] **7.3 Graceful Drain & Containerization:**
  - Drain signal broadcasts system chat notice, refuses new matches, waits up to 15 min for running matches to complete before shutdown.
  - Multi-stage `Dockerfile` producing a non-root `distroless` image (~30 MB).
- [ ] **7.4 CI/CD, Observability & Launch Validation:**
  - GitHub Actions (Workload Identity Federation → Artifact Registry `europe-west3` → Compute Engine `e2-small` VM + Cloud SQL private IP + Cloudflare Origin TLS / rustls).
  - `tracing` JSON logs → Cloud Logging + Cloud Monitoring uptime check on `/status`.
  - Playwright two-browser E2E suite + 200-concurrent-user load test.

**Exit Criteria:** Zero-disruption container deploy passes health checks, E2E suite, and 200-user load test, ready for closed beta invite links.

---

## 6. Verification & Quality Gates

Run at every milestone boundary:

```bash
# 1. Formatting & strict pedantic clippy (Native + WASM)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --target wasm32-unknown-unknown -p boomer-domain -p boomer-protocol -p boomer-client -- -D warnings

# 2. Unit & property-based tests
cargo test --workspace

# 3. Native vs. WASM Pool determinism gate (Milestone 2+)
cargo test -p boomer-domain --test pool_determinism
wasm-pack test --node crates/boomer-domain --test pool_determinism

# 4. Frontend typecheck, build & gzip bundle check (<= 800 KB)
cd web && npm run typecheck && npm run build
```

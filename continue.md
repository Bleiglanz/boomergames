# Boomergames — Session Handoff & Continuation Plan (`continue.md`)

**Last updated:** 2026-11-26  
**Repository:** `https://github.com/Bleiglanz/boomergames` (public, branch `main`)  
**Workspace path:** `/home/antonr/AntonObsidian/Boomergames`  
**Live production site:** `https://games.bleiglanz.info`  
**Health endpoint:** `https://games.bleiglanz.info/status`  
**Privacy policy (plain text, Rust-served):** `https://games.bleiglanz.info/privacy`  

---

## 1. Project Identity & Non-Negotiable Principles

**Boomergames** is a 100% free, ad-free, cookie-free classic **Checkers (English Draughts)** and **8-Ball Pool** portal inspired by classic 2000s Yahoo! Games:
- **Zero cookies:** Auth session tokens, guest reconnect tokens, and UI preferences (language, sound mute, high contrast, square numbers) live exclusively in browser `localStorage`.
- **Single deterministic core:** All game rules, Swept-CCD 8-Ball Pool physics (`f64` + `libm`), Checkers bitboard rules, and AI live in Rust (`boomer-domain`), compiled natively for the authoritative server (`boomer-server`) and to WebAssembly for the browser (`boomer-client`).
- **Browser-first, mobile/tablet-ready:** Full desktop 90%-width layout + responsive mobile/tablet touch layouts (`>= 44px` touch targets, touch gestures, pinch/orbit in 3D fullscreen).
- **Zero-warnings quality bar:** Always keep `cargo fmt --all -- --check`, native + `wasm32` `clippy --all-targets -- -D warnings`, `tsc --noEmit`, `vite build`, `cargo test --workspace --release`, and `make determinism` passing before commits.

---

## 2. Everything Completed Up To Now

### Step 1 — Offline Single-Player & Core Engine (`ca5bbfe` .. `0b22fdb`)
- **Milestone 0 (`ca5bbfe`):** Rust 2024 workspace (`boomer-domain`, `boomer-protocol`, `boomer-client`, `boomer-server`), strict `clippy::pedantic` + `#![forbid(unsafe_code)]`, Vite + TypeScript + Three.js shell.
- **Milestone 1 (`5b51b2d`):** Checkers (English Draughts) bitboard engine, perft-verified mandatory captures & multi-jumps, 3 time-sliced AI levels, interactive Three.js board (click, drag, flip, 1–32 numbering, move list, clocks, draw/resign/rematch).
- **Milestone 2 (`cc3da47`):** 8-Ball Pool Swept-CCD `f64`+`libm` physics, WPA rules, English spin, ball-in-hand, call-8 pocket, slingshot aiming/shooting, fine aim slider, vertical power bar, 3 time-sliced AI levels, and 1,000-shot bit-identical native/WASM determinism (`0x3374_7dcc_07bd_cadf`).
- **Milestone 3 (`8d6564e`, `0b22fdb`):** Procedural Web Audio synthesizer (`SoundSynth`), zero-cookie `localStorage` preferences, full EN/DE localization, WCAG 2.2 AA high-contrast mode, keyboard controls, and GitHub Actions CI + GitHub Pages static build.

### Step 2 — Authoritative Multiplayer & Resilience (`0a1b3e4` .. `5ccec63`)
- **Milestone 4 (`0a1b3e4`):** `boomer-server` (`axum` + `tokio`), strict CSP SPA hosting, cached `GET /status`, `GET /ws`, 4-table PostgreSQL schema (`migrations/0001_initial.sql`: `users`, `player_stats`, `matches`, `reports`), stateless HMAC session/guest/magic-link tokens, Brevo/SMTP port-587 STARTTLS mailer, Google ID token verifier.
- **Milestone 5 (`8440bac`):** Acyclic actor runtime (`LobbyHub` → `LobbyActor` → `TableActor`), non-blocking `ClientSink` (`OUTBOUND_QUEUE_CAP = 256`), auto-overflow rooms, lobby & table chat with profanity filter/rate limit/ignore, 30s invites, `Play Now` matchmaking, direct `#table=<id>` links, responsive Online Lounge UI.
- **Milestone 6 (`9b9ce62`):** Authoritative Checkers & Pool game sessions inside `TableActor`, `serde_json` `float_roundtrip` for 0-ULP bit-identical live Pool replays over WebSocket, 3 always-open 🤖 House Bot tables per room on bounded `spawn_blocking` pool, Elo rating + anti-farming decay + atomic High Scores.
- **Milestone 7 (`5ccec63`):** 45s disconnect grace (`PAUSED_DISCONNECT`) + automatic reconnect snapshot restore, moderation (`report`, `mod.mute`, `mod.ban`, `mod.close_table`), `SIGTERM` 15-min graceful drain, multi-stage distroless `Dockerfile`, CI deploy workflow, and 200-concurrent-user WebSocket load test (57/57 release tests passing).

### Live Online Gameplay, Production Launch & 3D Full-Screen Polish (`a5a2b91` .. `a52d308`)
- **Live Online Checkers & Pool Canvas (`a5a2b91`):** Built `BridgeTransport` + `BoomerRemoteClient` + `RemoteWasmBridge` so online matches immediately mount the full Three.js Checkers board or 8-Ball Pool table with turn locking, real player names, spectator mode, inline Table Chat, Rematch, Draw offers, and Resign.
- **Production Deployment on GCP VM `bleiglanzinfo` (`34.82.12.140`):**
  - Installed and low-RAM-tuned PostgreSQL 17 (`shared_buffers = 32MB`, `max_connections = 25`, `work_mem = 2MB`) with dedicated `boomergames` role/DB; SQLx migrations applied automatically.
  - Deployed `/opt/boomergames/boomer-server` + `/opt/boomergames/dist` under unprivileged `boomergames` user with `systemd` unit `boomergames.service` bound to `127.0.0.1:8085` (~6–8 MiB RSS peak).
  - Configured isolated `nginx` reverse proxy `/etc/nginx/sites-available/boomergames` for `games.bleiglanz.info` with WebSocket `Upgrade` headers and Let's Encrypt HTTPS/WSS cert (valid through `2027-02-22`, auto-renewal active), leaving the existing Flarum forum on `bleiglanz.info` completely untouched.
- **One-Command Deploy Script (`a9c736c`):** Added executable `./deploy.sh` (and `make deploy`) for build → tar → `gcloud compute scp` → atomic swap → `systemctl restart` → live `/status` check.
- **Authentication & Nicknames (`a9c736c`, `64d47cd`, `269edda`):**
  - Added Google Sign-In UI (lazy-loaded Google Identity Services, `googleClientId` exposed on `/status`, CSP updated) — live & verified with Google login!
  - Configured Brevo SMTP (`smtp-relay.brevo.com:587`) and fixed IONOS DNS (`_dmarc` TXT, merged SPF with `include:spf.brevo.com`, restored `games.bleiglanz.info` `A -> 34.82.12.140`).
  - Removed 30-day rename cooldown so every login email remembers its last saved nickname and prompts the user to keep or change it on every login.
  - Added bilingual plain-text (zero HTML, Rust-served) `GET /privacy` endpoint + footer link for Google OAuth compliance.
- **Reconnect Bug Fix (`712aa71`):** Cleared stale client-side `activeTable` on reconnect/login/table removal and avoided leaving a paused table when rejoining the same room.
- **Widescreen Desktop Layout (`69004dc`):** Expanded desktop `.main-area` to `90%` width (5% margins, `max-width: 2400px`) with viewport-height capping for a much larger Pool table and Checkers board.
- **Continuous Full-Screen Pool Mode + Hidden 3D Pool Table (`831278e`, `a52d308`):**
  - Continuous toggle (`⛶ Full Screen (F)` / `✕ Exit Full Screen` / `F` / `Esc`) mid-game without interrupting matches or WebSockets.
  - Starts strictly orthogonal straight down `-Z` with flat lighting (looks identical to 2D top-down).
  - Right-drag / Middle-drag / Shift+drag / Scroll / 2-finger touch / `W A S D` smoothly morphs from `OrthographicCamera` → `PerspectiveCamera` (singularity-free spherical orbit, zero jump) and reveals a full 3D mahogany table, raised cushions, open-throat flush pockets with extruded brass corner & side castings, recessed drop chutes, mahogany apron & carved legs, hardwood floor, overhead billiard-hall lighting, tapered 3D cue stick, and physically rolling 3D spherical balls (`SphereGeometry`) with `THREE.Raycaster` felt-plane aiming/shooting from any angle.
- **Phase A, Phase B & Profile Modal Polish (2026-11-26):**
  - **Clean WebSocket Shutdown on Drain/Deploy:** Added `LobbyHub::disconnect_all_clients()` + `shutdown_notified()` (waking both idle lobby sockets and pre-handshake `/ws` sockets) called at the end of `graceful_drain_signal()`, verified by integration test `graceful_shutdown_disconnects_idle_ws_clients_immediately`.
  - **Brevo Magic-Link Dispatch Logging:** `LettreMailSender` logs `info!(smtp_host, recipient_domain, "dispatched magic-link email")` on successful SMTP send without leaking tokens or user addresses.
  - **VM Ops Hardening (`bleiglanzinfo`):** Provisioned persistent 1 GB `/swapfile` (`vm.swappiness=10`) and nightly PostgreSQL backup (`boomergames-backup.timer` → `/usr/local/sbin/boomergames-pg-backup.sh` → `/var/backups/boomergames/` with 7-day retention).
  - **3D Pool Pocket Drop Animation & Behind-Cue Camera Preset:** Purely visual 180 ms pocket chute drop-down animation (zero change to deterministic physics), `🎯 Behind Cue (C)` camera preset swinging behind the cue stick along the shot line, clamped low-angle raycast, forgiving 3D ball-in-hand grab radius, and zero-allocation per-frame background blending.
  - **Checkers Continuous Full-Screen Mode & Hidden 3D Carved Wooden Board:** Mirrored `⛶ Full Screen (F)` / `F` / `Esc` toggle for Checkers with the same secret 2D→3D fullscreen reveal (right-drag / Shift+drag / scroll / 2-finger touch / `WASD` / `R` reset): carved walnut frame & stepped plinth, grained inlaid maple/walnut squares, fluted 3D wooden checkers, physically stacked double-height 3D Kings with brass collar & crown medallions, and parlor tabletop lighting.
  - **Audio Polish & Master Volume Slider:** Added persisted zero-cookie `volume` slider in the header, dynamics compressor to prevent break-scatter clipping, reusable noise buffer, and upgraded procedural acoustics (chalked cue hit, jittered phenolic ball clicks, rubber cushion thuds, hollow pocket chute rattle, tactile hardwood checker clacks, and crowning triad).
  - **3D Pool Cue Alignment & Rail Clearance Fix:** Anchored the cue pivot at exact ball-center height (`z = BALL_RADIUS`) with positive axial elevation (`cuePitchGroup`) dynamically clearing cushions, mahogany rails, brass castings, and rear balls without clipping or aim-line angle mismatch.
  - **Player Profile Modal & Match History UI:** Clicking any registered player in the Lounge Roster, High Scores table, or your own Account modal opens a classic Player Profile modal backed by `profile.get` (Pool & Checkers Elo, W/L/D stats, member-since date, and last 20 matches).

---

## 3. Codebase Map & Critical Invariants

### Key Files
- **Domain (pure deterministic Rust):**
  - `crates/boomer-domain/src/checkers/` — bitboard rules, move gen, time-sliced AI
  - `crates/boomer-domain/src/pool/` — Swept-CCD `f64`+`libm` physics, WPA rules, aim preview, time-sliced AI
  - `crates/boomer-domain/src/rating.rs` — Elo, provisional K-factor, anti-farming decay, high-score eligibility
- **Protocol:** `crates/boomer-protocol/src/{envelope,auth,lobby,checkers,pool}.rs`
- **WASM Client:**
  - `crates/boomer-client/src/{app,animation,local_adapter,bridge_adapter,wasm}.rs`
- **Authoritative Server:**
  - `crates/boomer-server/src/main.rs` — entrypoint, DB/SMTP/Google wiring, graceful drain
  - `crates/boomer-server/src/config.rs` — env config
  - `crates/boomer-server/src/state.rs` — `AppState`, `StatusPayload`
  - `crates/boomer-server/src/adapters/{http,ws,db,memory,mail,google}.rs`
  - `crates/boomer-server/src/actors/{hub,lobby_actor,table_actor,game_session}.rs`
  - `migrations/0001_initial.sql` — PostgreSQL schema
- **Frontend (`web/`):**
  - `web/src/main.ts` — app shell, offline/online bridge switching, Pool fullscreen toggle, HUD
  - `web/src/wasm_bridge.ts` — `WasmBridge` & `RemoteWasmBridge`
  - `web/src/online/session.ts` — WebSocket client, reconnect, auth, game-frame routing
  - `web/src/ui/lobby.ts` — Online Lounge, Table Room, Create Table / Account / High Scores modals
  - `web/src/render/{renderer,pool_scene,checkers_scene}.ts` — Three.js stage, 3D Pool table, Ortho↔Perspective orbit camera
  - `web/src/audio/synth.ts` — Web Audio procedural SFX
  - `web/src/i18n/strings.ts` — EN/DE translations
  - `web/src/styles/classic.css` — retro classic theme, 90% desktop layout, responsive mobile tabs, fullscreen HUD
- **Operations:** `deploy.sh`, `Makefile`, `Dockerfile`, `.github/workflows/`

### Critical Invariants (Do Not Break!)
1. **Keep `serde_json` `features = ["float_roundtrip"]`** in root `Cargo.toml` — required for 0-ULP bit-identical 15-ball Pool break replays over JSON WebSockets.
2. **Keep `OUTBOUND_QUEUE_CAP = 256`** in `crates/boomer-server/src/adapters/ws.rs` — prevents 200-user room join bursts from triggering slow-client disconnects.
3. **Never add HTML cookies or third-party tracking.** Keep Google Identity Services script lazy-loaded only when the user opens the Sign In modal.
4. **Never touch `/etc/nginx/sites-available/default` or `mariadbd`** on `bleiglanzinfo` (that hosts the user's Flarum forum at `https://bleiglanz.info`).

---

## 4. Production Infrastructure & Operations Cheat Sheet

- **GCP Project:** `rustchat-491606` (account `antonrechenauer@gmail.com`)
- **GCP CLI path:** `/home/antonr/google-cloud-sdk/bin/gcloud` (not in default `$PATH`)
- **VM:** `bleiglanzinfo`, zone `us-west1-b`, `e2-micro` (`964 MiB` RAM, `1 GiB` `/swapfile` with `vm.swappiness=10`, `20G` disk with ~`12G` free), IP `34.82.12.140`, Debian 13 (trixie, `GLIBC 2.41` vs local Ubuntu `GLIBC 2.39` — locally built `x86_64` binaries run natively!).
- **SSH key:** `~/.ssh/google_compute_engine` (already configured & propagated).
- **VM paths:**
  - Binary + static SPA: `/opt/boomergames/boomer-server`, `/opt/boomergames/dist/` (owned by `boomergames:boomergames`)
  - Secrets/env (`chmod 600`): `/etc/boomergames/env` (contains `BOOMER_BIND`, `BOOMER_STATIC_DIR`, `BOOMER_PUBLIC_ORIGIN`, `BOOMER_SESSION_SECRET`, `DATABASE_URL`, `BOOMER_LOG_FORMAT`, `GOOGLE_CLIENT_ID`, `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM` — **never commit or print secret values**)
  - Systemd unit: `/etc/systemd/system/boomergames.service` (listens `127.0.0.1:8085`)
  - Nginx vhost: `/etc/nginx/sites-available/boomergames` → `sites-enabled/boomergames` (TLS managed by Certbot)
  - PostgreSQL 17 tuning: `/etc/postgresql/17/main/conf.d/99-boomergames-micro.conf`
- **Common commands:**
  - Local dev (frontend HMR): `cd web && npm run dev`
  - Local full server: `cargo run --release -p boomer-server` (`http://127.0.0.1:8080`)
  - Full verify: `cargo fmt --all -- --check && make clippy && make build && cargo test --workspace --release && make determinism`
  - Deploy to GCP: `./deploy.sh` (or `./deploy.sh --skip-build` if already built, or `make deploy`)
  - Check live logs: `/home/antonr/google-cloud-sdk/bin/gcloud compute ssh bleiglanzinfo --zone=us-west1-b --command='sudo journalctl -u boomergames -n 40 --no-pager'`

---

## 5. Remaining Manual QA & Next Steps

### Interactive / Production Verification (User-Driven)
1. **Verify Brevo Magic-Link delivery end-to-end:**
   - Trigger a real magic-link sign-in on `https://games.bleiglanz.info`, confirm inbox receipt, `#login=...` sign-in, nickname persistence across logins, and verify the new `dispatched magic-link email` log entry in `journalctl -u boomergames`.
2. **Full Live Two-Player & Mobile/Tablet QA Pass:**
   - Play a full rated 8-Ball Pool match and Checkers match end-to-end between two accounts (desktop + smartphone/tablet) on `https://games.bleiglanz.info`:
     - Verify Elo updates, High Scores table, Player Profile modal, Rematch, Draw offers, Resign, 45s disconnect pause/resume, Spectator mode, and Table Chat.
     - Test mobile landscape & portrait ergonomics in normal and Full-Screen 3D modes (2-finger orbit/zoom vs 1-finger slingshot, Behind Cue button, pocket drop animation).
3. **Google OAuth Consent Screen Verification Check:**
   - Confirm in Google Cloud Console that the OAuth consent screen is published to **Production** (so any external Gmail friend can sign in without being on a test-user allowlist) and has `https://games.bleiglanz.info/privacy` set as the Privacy Policy URL.

### Future Expansion Ideas (Backlog)
- Spectator count badge & live spectator list inside active online match HUD.
- Optional third classic game (e.g., **Backgammon**, **Connect Four**, or **9-Ball Pool** reusing the exact `boomer-domain` Swept-CCD physics engine!).

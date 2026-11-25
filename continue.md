# Boomergames — Session Handoff & Continuation Plan (`continue.md`)

**Last updated:** 2026-11-25  
**Repository:** `https://github.com/Bleiglanz/boomergames` (public, branch `main`)  
**Workspace path:** `/home/antonr/AntonObsidian/Boomergames`  
**Latest commit (`HEAD == origin/main`):** `a52d308`  
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
- **VM:** `bleiglanzinfo`, zone `us-west1-b`, `e2-micro` (`964 MiB` RAM, ~`336 MiB` available, `0` swap, `20G` disk with `13G` free), IP `34.82.12.140`, Debian 13 (trixie, `GLIBC 2.41` vs local Ubuntu `GLIBC 2.39` — locally built `x86_64` binaries run natively!).
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

## 5. Known Rough Edges & Observations To Address Next

1. **Graceful shutdown with idle WebSocket clients:**
   - `axum::serve(...).with_graceful_shutdown(graceful_drain_signal(drain_hub))` waits for existing open WebSocket connections to close AFTER `graceful_drain_signal` finishes. Currently `systemctl stop boomergames.service` could wait up to `TimeoutStopSec=900` if a browser leaves a WebSocket open during a deploy!
   - **Fix needed:** After `graceful_drain_signal` completes (0 active games or deadline reached), actively disconnect all registered WebSocket sinks (`hub.force_disconnect_all()`) with a clean Close frame so `axum::serve` exits in `< 1s` every time!
2. **Verify Brevo Magic-Link delivery end-to-end:**
   - Brevo SMTP (`smtp-relay.brevo.com:587`) is configured and initialized cleanly, and IONOS DNS (`SPF`, `DKIM`, `_dmarc` TXT, `games` A record) is green.
   - Add a `tracing::info!` on successful magic-link SMTP send (logging only destination domain or sanitized status, never the login token!) so `journalctl` confirms delivery at a glance, and verify a real inbox delivery test.
3. **3D Pool Pocket Drop Animation & Visual Polish:**
   - Pockets are now genuinely open-throated and flush with the cloth, with recessed drop chutes. Right now pocketed balls disappear immediately when the domain marks `pocketed = true`; in 3D view, a quick 180ms visual drop-down animation (`z: BALL_RADIUS -> -0.06`) into the pocket chute before hiding would make potting balls in 3D feel phenomenal!
   - Also check the 3D aim line / cue-ball ball-in-hand drag feel at low dramatic camera angles (`tilt > 55°`) and consider adding a tiny subtle **camera angle preset** (e.g., "Behind Cue" shot camera button in 3D fullscreen!).
4. **Add small swap file on `e2-micro` (optional ops safety):**
   - `bleiglanzinfo` currently has `964 MiB` RAM and `0B` swap (`336 MiB` available). Adding a 1 GB swapfile (`/swapfile`, `vm.swappiness=10`) prevents any future OOM spike during `apt upgrade` or traffic bursts across Flarum + MariaDB + PostgreSQL + Boomergames.

---

## 6. Carefully Planned Next Steps (Roadmap for Next Session)

### Phase A — Reliability & Quick Wins (Highest Priority, ~30–45 min)
1. **Fix `SIGTERM` / deploy shutdown hang with open WebSockets:**
   - Add `LobbyHub::disconnect_all_clients()` called right at the end of `graceful_drain_signal()` in `crates/boomer-server/src/main.rs` (after all active games finish draining), plus an integration test verifying immediate clean shutdown when idle WS clients are connected.
2. **Confirm Brevo Magic-Link email delivery + add success log:**
   - Log `info!(host = ..., "dispatched magic-link email")` (without leaking the secret token!) in `crates/boomer-server/src/adapters/mail.rs`.
   - Test a real magic-link sign-in on `https://games.bleiglanz.info` (confirm inbox receipt, `#login=...` sign-in, nickname persistence across logins).
3. **Optional 1 GB swapfile + nightly PostgreSQL backup on `bleiglanzinfo`:**
   - Create `/swapfile` (`1G`, `swappiness=10`) for OOM resilience on `e2-micro`.
   - Add a lightweight `pg_dump boomergames | gzip` cron/systemd timer keeping last 7 days in `/var/backups/boomergames/`.

### Phase B — 3D Pool Polish & Checkers Full-Screen (~1–2 hours)
1. **3D Pocket Drop Animation (`web/src/render/pool_scene.ts`):**
   - Animate newly pocketed balls smoothly dropping down into the recessed pocket chute (`z: BALL_RADIUS -> -0.065`) over ~160 ms before hiding, purely visual in the renderer (zero change to deterministic physics!).
2. **Optional "Behind Cue" 3D Camera Preset (`web/src/render/renderer.ts`):**
   - Add a toggle/key (e.g., `C` key or button in fullscreen bar when in 3D) that swings the 3D camera directly behind the cue stick looking down the shot line — super fun for lining up tough cuts!
3. **Checkers Continuous Full-Screen Mode (optional):**
   - Mirror the same continuous `⛶ Full Screen (F)` toggle for Checkers (and optionally a hidden 3D carved wooden Checkers board & stacked king pieces reveal!).
4. **3D Pool Lighting / Polish Details:**
   - Fine-tune 2D vs 3D ball outline contrast, overhead lamp reflection feel, and verify `three` bundle chunking (note Vite's minor 506 kB chunk size notice — optional `manualChunks` polish).

### Phase C — Live Multiplayer Playtesting, Mobile QA & Social Features (~1–2 hours)
1. **Full Live Two-Player & Mobile/Tablet QA Pass:**
   - Play a full rated 8-Ball Pool match and Checkers match end-to-end between two accounts (desktop + smartphone/tablet) on `https://games.bleiglanz.info`:
     - Verify Elo updates, High Scores table, Rematch, Draw offers, Resign, 45s disconnect pause/resume, Spectator mode, and Table Chat.
     - Test mobile landscape & portrait ergonomics in both normal and Full-Screen 3D modes (2-finger orbit/zoom vs 1-finger slingshot).
2. **Player Profile Modal & Match History UI:**
   - The server already supports `profile.get` (returning ratings, wins, and last 20 matches), so clicking any player name in the Lobby Roster or High Scores can open a crisp classic **Player Profile** modal showing their Checkers & Pool Elo, win/loss record, and recent matches!
3. **Google OAuth Consent Screen Verification Check:**
   - Confirm in Google Cloud Console that the OAuth consent screen is published to **Production** (so any external Gmail friend can sign in without being on a test-user allowlist) and has `https://games.bleiglanz.info/privacy` set as the Privacy Policy URL (skipping the app logo avoids Google's lengthy brand-verification review).

### Phase D — Future Expansion Ideas (Backlog)
- Sound volume slider & richer subtle cue/pocket acoustics.
- Spectator count badge & live spectator list inside active online match HUD.
- Optional third classic game (e.g., **Backgammon**, **Connect Four**, or **9-Ball Pool** reusing the exact `boomer-domain` Swept-CCD physics engine!).

---

## 7. Next Session Kickoff Checklist

When starting the next session, tell the assistant:
> "Read `continue.md` and let's start with Phase A (clean WebSocket shutdown on deploy, Brevo magic-link verification, and VM swap/backup) — or jump straight into Phase B (3D pocket drop animation & behind-cue camera)."

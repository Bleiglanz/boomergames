# Boomergames

A classic 2000s Yahoo! Games–inspired browser game lounge featuring **8-Ball Pool**, **Checkers (English Draughts)**, and **Chess**.

- **100% free · No ads · No cookies · No tracking**
- **Playable offline & online**: runs completely client-side in Rust/WebAssembly (including single-player AI) or connects to the authoritative Rust multiplayer lounge server for live rooms, tables, Elo ratings, High Scores, chat, and spectating.
- **Live instance**: [games.bleiglanz.info](https://games.bleiglanz.info)

---

## Features

| Game | Offline Single-Player & Hotseat | Online Multiplayer Lounge | Visuals & Controls |
| --- | --- | --- | --- |
| **8-Ball Pool** | Hotseat, vs. Bot (Easy / Medium / Hard), Free Practice | Public/Protected/Private tables, House Bots, Elo, High Scores, Spectating | 0-ULP deterministic Swept-CCD physics, slingshot drag or keyboard aim, English spin, aiming aids, Full-Screen hidden 3D pool table (`F`) |
| **Checkers** *(English Draughts)* | Hotseat, vs. Bot (Easy / Medium / Hard) | Rooms, tables, clocks, House Bots, Elo, High Scores, Spectating | Mandatory captures, multi-jumps, 1–32 notation toggle, flip board, Full-Screen hidden 3D carved wooden board & stacked Kings (`F`) |
| **Chess** | Hotseat, vs. **Stockfish 19** (`nmrugg/stockfish.js` Easy / Medium / Hard) | Rooms, tables, clocks, House Bots, Elo, High Scores, Spectating | Full FIDE rules (castling, en passant, promotions, threefold repetition, 50-move rule, insufficient material), SAN move list, clocks, flip board, Full-Screen hidden 3D sculpted Staunton pieces (`F`) |

Additional polish across the whole app:
- **Bilingual UI**: English (`en`) and German (`de`).
- **Accessibility**: WCAG High-Contrast theme toggle, keyboard navigation, ARIA live announcements.
- **Audio**: Procedural Web Audio acoustics with master volume slider + optional CC-BY 4.0 cool jazz lounge background music.
- **Zero-cookie authentication (Online Lounge)**: Guest play out of the box, optional passwordless Email Magic Link (SMTP) and lazy-loaded Google Sign-In.

---

## Architecture

Boomergames follows a **hexagonal (ports & adapters) architecture** where every game rule, physics step, and rating formula lives in pure Rust and compiles identically to native x86_64/ARM64 (server) and `wasm32-unknown-unknown` (browser):

```text
Boomergames/
├── crates/
│   ├── boomer-domain/     # Pure deterministic game engines (Pool, Checkers, Chess), AI, Elo math
│   ├── boomer-protocol/   # Versioned JSON wire DTOs (ClientMessage / ServerMessage)
│   ├── boomer-client/     # Transport-agnostic WASM state machine (offline LocalSessionAdapter & online BridgeTransport)
│   └── boomer-server/     # Axum HTTP/WebSocket server, actor-per-room/table Tokio engine, Postgres & in-memory adapters
├── migrations/            # Embedded sqlx PostgreSQL schema migrations
├── web/                   # Thin TypeScript + Vite + Three.js renderer, Stockfish 19 Web Worker, Web Audio
│   ├── public/audio/      # CC-BY 4.0 cool jazz MP3 tracks
│   ├── public/stockfish/  # Auto-copied from node_modules/stockfish during npm dev/build (gitignored)
│   └── src/
│       ├── audio/         # Procedural sound synth + jazz music player
│       ├── chess/         # Stockfish 19 Web Worker wrapper (UCI)
│       ├── i18n/          # EN / DE string tables
│       ├── online/        # WebSocket OnlineSession controller
│       ├── render/        # Three.js StageRenderer + PoolScene, CheckersScene, ChessScene
│       ├── styles/        # Classic 2000s lounge CSS + high-contrast theme
│       └── ui/            # Online Lounge, Table Room, High Scores, Account & Profile UI
├── Dockerfile             # Multi-stage distroless production image (~30 MB)
├── Makefile               # Common check, build, test, and determinism targets
└── deploy.sh              # Release build + VM deploy helper
```

### Why `stockfish-19-lite-single`?
Single-player Chess uses the single-threaded lite WASM build from `nmrugg/stockfish.js` (`stockfish-19-lite-single.js` + `stockfish-19-lite-single.wasm`, ~1.8 MB) inside a dedicated Web Worker. Unlike multi-threaded WASM builds, it requires **no `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy` headers**, keeping Google Sign-In and static hosting (such as GitHub Pages) working seamlessly. If a restricted browser blocks the Web Worker, `boomer-client` transparently falls back to its built-in Rust alpha-beta Chess engine.

---

## Prerequisites

Install the following toolchains before building:

1. **Rust** (stable toolchain — `rust-toolchain.toml` automatically selects `clippy`, `rustfmt`, and the `wasm32-unknown-unknown` target via `rustup`):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **`wasm-pack`** (compiles `crates/boomer-client` to `web/src/wasm/`):
   ```bash
   cargo install wasm-pack --locked
   ```
3. **Node.js 20+ and npm** (for Vite, TypeScript, Three.js, and `stockfish`).
4. **PostgreSQL 15+** *(optional)*: only needed if you want persistent accounts/ratings/match history locally. Without `DATABASE_URL`, `boomer-server` automatically starts with in-memory repositories.

---

## Getting Started After Cloning

```bash
# 1. Clone the repository
git clone https://github.com/Bleiglanz/boomergames.git
cd boomergames

# 2. Install frontend dependencies (including Three.js and nmrugg/stockfish.js)
cd web && npm install && cd ..

# 3. Build the Rust/WASM client and production web bundle
#    (automatically copies Stockfish 19 JS/WASM into web/public/stockfish/ and web/dist/stockfish/)
make build
```

> **Note:** Generated artifacts (`web/src/wasm/`, `web/public/stockfish/`, `web/dist/`, and `target/`) are gitignored. Running `cd web && npm install` followed by `make build` (or `make dev`) recreates all of them automatically.

### Running Locally

- **Offline / Frontend Hot-Reload Dev Server (Vite):**
  ```bash
  make dev
  ```
  Opens the Vite dev server (typically `http://localhost:5173`). Offline Pool, Checkers, and Stockfish Chess work immediately without a backend.

- **Full Online Lounge + Offline App (Single Local Server):**
  ```bash
  make build
  cargo run -p boomer-server
  ```
  Serves both the built SPA (`web/dist`) and the live WebSocket multiplayer server at `http://127.0.0.1:8080` using in-memory dev storage (no PostgreSQL setup required).

---

## Testing & Quality Checks

The workspace enforces strict Rust lints (`unsafe_code = "deny"`, `clippy::pedantic = "deny"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `indexing_slicing = "deny"`).

```bash
# Formatting check
make fmt

# Clippy for both native workspace and wasm32 client crates (warnings = errors)
make clippy

# Full unit + integration test suite (release mode recommended for Pool physics tests)
cargo test --workspace --release

# 1,000-shot native vs. WASM 0-ULP Pool physics determinism check
make determinism

# Frontend TypeScript check & Vite bundle
cd web && npm run typecheck && npm run build
```

---

## Configuration (Environment Variables)

`boomer-server` reads configuration from environment variables (see `crates/boomer-server/src/config.rs`). All variables have safe local-development defaults:

| Variable | Default | Description |
| --- | --- | --- |
| `BOOMER_BIND` | `127.0.0.1:8080` | Socket address to listen on (e.g. `127.0.0.1:8085` behind Nginx or `0.0.0.0:8080` in Docker). |
| `BOOMER_STATIC_DIR` | `web/dist` | Path to the compiled Vite SPA directory. |
| `BOOMER_PUBLIC_ORIGIN` | `http://127.0.0.1:8080` | Public HTTPS origin used when generating magic-link login URLs. |
| `BOOMER_SESSION_SECRET` | *(dev fallback)* | **Required in production.** Strong random HMAC secret for stateless session, magic-link, and guest tokens. |
| `DATABASE_URL` | *(unset → in-memory)* | PostgreSQL connection URL. Embedded SQL migrations in `migrations/` run automatically on startup. |
| `GOOGLE_CLIENT_ID` | *(unset → disabled)* | Google OAuth 2.0 Web Client ID for optional Google Sign-In. |
| `SMTP_HOST` | *(unset → disabled)* | SMTP relay host for passwordless magic-link emails (STARTTLS on port 587). |
| `SMTP_PORT` | `587` | SMTP relay port. |
| `SMTP_USER` | `""` | SMTP relay username. |
| `SMTP_PASS` | `""` | SMTP relay password / API key. |
| `SMTP_FROM` | `Boomergames <noreply@boomergames.local>` | Verified sender address (`From:`) matching your SMTP provider's SPF/DKIM domain. |
| `BOOMER_LOG_FORMAT` | *(text)* | Set to `json` for structured JSON logs in production. |

> Never commit `.env` files, keys, or secrets. `.env` and `.env.*` are gitignored.

---

## Deployment

### Option 1: Docker
Build and run the minimal non-root distroless container (~30 MB):
```bash
docker build -t boomergames .
docker run --rm -p 8080:8080 \
  -e BOOMER_PUBLIC_ORIGIN=https://games.example.com \
  -e BOOMER_SESSION_SECRET="$(openssl rand -hex 32)" \
  -e DATABASE_URL="postgres://user:pass@host:5432/boomergames" \
  boomergames
```

### Option 2: Linux VM behind Nginx + systemd
1. Build with `make build && cargo build --release -p boomer-server`.
2. Deploy `target/release/boomer-server` and `web/dist` behind an HTTPS reverse proxy (Nginx) forwarding `/ws` with WebSocket upgrade headers (`Upgrade` / `Connection`) and `/` to `boomer-server`.
3. Provide environment variables via a root-only systemd `EnvironmentFile` (e.g. `/etc/boomergames/env` with mode `0600`).
4. For the reference GCP VM setup, `./deploy.sh` automates build, tarball upload, atomic swap, systemd restart, and `/status` verification (supports `BOOMER_VM_NAME`, `BOOMER_VM_ZONE`, `BOOMER_SITE_URL`, and `--skip-build`).

---

## How to Extend Boomergames with a New Game

Because every game follows the same hexagonal boundaries, adding a fourth game (e.g. Backgammon, Reversi, or Connect Four) is straightforward:

1. **Pure Domain (`crates/boomer-domain`)**:
   - Add `src/<game>/{board,rules,ai,mod}.rs` with zero I/O, deterministic `SplitMix64` RNG, and unit tests.
   - Add `GameKind::<Game>` in `src/lib.rs` and its minimum High Score activity threshold in `src/rating.rs`.
2. **Wire Protocol (`crates/boomer-protocol`)**:
   - Add `src/<game>.rs` with `SettingsDto`, `StateDto`, `OutcomeDto`, etc.
   - Add `GameKindDto::<Game>` plus `ClientMessage` / `ServerMessage` variants in `src/envelope.rs`.
3. **Database Migration (`migrations/`)**:
   - Add `migrations/000N_add_<game>.sql` updating the `CHECK (game IN (...))` constraints on `player_stats` and `matches`.
4. **Authoritative Server (`crates/boomer-server`)**:
   - Wire conversions in `src/mapping.rs` and `src/adapters/memory.rs`.
   - Route incoming move messages in `src/adapters/ws.rs`.
   - Open a default lounge room in `src/actors/hub.rs` (automatically spawns 3 House Bot tables).
   - Add an `ActiveGame::<Game>` / `BotWorkRequest::<Game>` variant in `src/actors/game_session.rs` and handle turns, clocks, draws, and bot moves in `src/actors/table_actor.rs`.
5. **Client WASM State Machine (`crates/boomer-client`)**:
   - Add offline session handling in `src/local_adapter.rs` and DTO conversions in `src/mapping.rs`.
   - Add `UiInput::<Game>*`, `<Game>View`, and `ScreenView::GameShell.<game>` in `src/app.rs`, plus `"<game>"` in `src/wasm.rs`.
6. **Thin Frontend Shell (`web/src/`)**:
   - Add TypeScript DTOs in `wasm_bridge.ts` and handle `<game>.snapshot` in `online/session.ts`.
   - Create `render/<game>_scene.ts` (implementing `set3DVisualBlend` if you want the hidden 3D Full-Screen reveal) and register it in `render/renderer.ts`.
   - Add localized EN/DE strings in `i18n/strings.ts`, High Scores/Profile labels in `ui/lobby.ts`, and the Home card + HUD in `main.ts`.
7. **Verify**:
   - Run `make fmt && make clippy && make build && cargo test --workspace --release`.

---

## Engineering Invariants

Contributors should preserve these invariants:
- Keep `serde_json` with `features = ["float_roundtrip"]` in root `Cargo.toml` so Pool shot replay stays 0-ULP bit-identical between native and WASM.
- Keep `OUTBOUND_QUEUE_CAP = 256` on per-connection WebSocket channels in `crates/boomer-server/src/adapters/ws.rs` to prevent slow clients from stalling rooms.
- Keep zero HTTP cookies and zero third-party trackers; only lazy-load Google Identity Services when the user opens the Sign-In modal.

---

## Credits & Third-Party Licenses

- **Chess Engine (Single-Player)**: [Stockfish 19](https://stockfishchess.org/) compiled to WebAssembly via [nmrugg/stockfish.js](https://github.com/nmrugg/stockfish.js), licensed under the [GNU General Public License v3.0 (GPL-3.0)](https://www.gnu.org/licenses/gpl-3.0.html).
- **Background Cool Jazz Music**:
  - ["Backbay Lounge"](https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1700001), ["Airport Lounge"](https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1100806), and ["Cool Vibes"](https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1100863) by Kevin MacLeod ([incompetech.com](https://incompetech.com/))
  - Licensed under [Creative Commons: By Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/) *(re-encoded to 96 kbps MP3)*.

# User Requirements Document (URD): Turn-Based Casual Multiplayer Platform

## 1. Document Overview

### 1.1 Purpose
This document defines the user and system requirements for a platform-agnostic, web-based multiplayer casual game portal (modeled after classic portals such as Yahoo! Games). The platform supports **turn-based, two-player games** (e.g., Billiards, Turn-Based Air Hockey, Chess) featuring a shared social lounge, player matchmaking via virtual "tables," direct player challenges, spectators, and integrated chat.

### 1.2 Scope
* **In Scope:** User presence, lounge/room chat, table creation and matchmaking, direct invitations, turn-based authoritative game state synchronization, spectating, and basic player statistics/ratings.
* **Out of Scope:** Real-time continuous physics/action games (twitch-based gameplay), voice/video chat, and payment/monetization processing.

### 1.3 User Roles
1. **Guest / Unauthenticated User:** Can view public lounges (optional, depending on auth policy) or is prompted to register/log in.
2. **Registered Player (In Lounge):** Authenticated user browsing the lobby, chatting, or seeking a match.
3. **Active Player (Seated):** A player occupying `Seat 1` or `Seat 2` at a game table.
   * **Active Turn Player:** The seated player whose turn it currently is to submit a game action.
   * **Waiting Player:** The seated player waiting for their opponent to complete their turn.
4. **Spectator:** Any user observing an active game table without occupying a player seat.

---

## 2. Functional Requirements: The Social Lounge (Lobby)

### 2.1 Lounge Entry & Presence
* **FR-1.1:** Upon successful login, the user shall be placed into a default **Lounge** (or presented with a list of themed/skill-tiered Lounges to join).
* **FR-1.2:** The Lounge interface shall display a real-time **User Roster** listing all users currently connected to that specific Lounge.
* **FR-1.3:** Each entry in the User Roster must display:
  * Username / Display Name
  * Skill Rating (e.g., numeric Elo score and/or rank badge)
  * Current Status (`Available`, `At Table - Waiting`, `In Game`, `Spectating`)
* **FR-1.4:** The User Roster must update automatically in real time as users enter, leave, or change status.

### 2.2 Lounge Chatroom
* **FR-2.1:** The Lounge shall feature a shared text chatroom visible to all users in that Lounge.
* **FR-2.2:** Users shall be able to type and broadcast text messages to the entire Lounge.
* **FR-2.3:** The system shall enforce basic rate-limiting (e.g., max 1 message per second) to prevent chat flooding.
* **FR-2.4:** Users shall have a client-side **"Ignore / Mute"** action that hides all lounge chat, table chat, and direct invites from a selected user.

---

## 3. Functional Requirements: Matchmaking & Tables

### 3.1 The Table List
* **FR-3.1:** The Lounge shall display an active list of **Game Tables**.
* **FR-3.2:** Each Table listing must display:
  * Table ID / Number
  * `Seat 1` status (Empty or Player Name + Rating)
  * `Seat 2` status (Empty or Player Name + Rating)
  * Table Visibility (`Public` or `Private/Invite-Only`)
  * Configured Game Rules (e.g., Turn Timer duration, Target Score)
  * Current Spectator Count
  * Action Button (`Join`, `Watch`, or `Full`)

### 3.2 Creating and Joining Tables
* **FR-4.1 (Create Table):** Any `Available` user can click **"Create Table"** to instantiate a new table, automatically placing themselves into `Seat 1` and setting their status to `At Table - Waiting`.
* **FR-4.2 (Table Configuration):** The table creator (`Seat 1`) may configure allowed match parameters (e.g., Public vs. Private, Turn Time Limit: 30s / 60s / Unlimited) before the match begins.
* **FR-4.3 (Join Table):** Any `Available` user can click **"Join"** on a `Public` table with an open seat to occupy `Seat 2`.
* **FR-4.4 (Quick Match / "Play Now"):** Users may click a **"Play Now"** button to be automatically seated into the highest-priority open `Public` table matching their approximate skill rating, or automatically create a new table if none are available.

### 3.3 Direct Invites (Challenges)
* **FR-5.1:** Any `Available` user can click on another `Available` user in the User Roster and select **"Invite to Game"**.
* **FR-5.2:** The target user shall receive a non-blocking modal notification displaying the challenger's name, rating, and proposed game rules, with **"Accept"** and **"Decline"** options.
* **FR-5.3:** Invites shall automatically expire and dismiss if not answered within 30 seconds.
* **FR-5.4:** Upon clicking **"Accept"**, the system shall automatically create a `Private` table, seat both players (`Seat 1` = Challenger, `Seat 2` = Accepter), and launch the Game Room view for both users.

---

## 4. Functional Requirements: The Game Room & Turn-Based Loop

### 4.1 Game Room Interface & Social
* **FR-6.1:** Entering a table (as a Player or Spectator) opens the **Game Room** view, which contains the Game Board/Canvas, Player Info Panels, Turn Timer, and a **Table Chatroom**.
* **FR-6.2:** The **Table Chatroom** is isolated to the occupants of that specific table (`Seat 1`, `Seat 2`, and all current `Spectators`).
* **FR-6.3:** Both `Seat 1` and `Seat 2` can freely send and receive messages in the Table Chatroom at any time during the match, regardless of whose turn it is.
* **FR-6.4 (Spectating):** Any user in the Lounge may click **"Watch"** on an active table to join as a Spectator. Spectators can view the live game state and participate in Table Chat (unless the table host toggles "Mute Spectators"), but have zero interactive control over the game board.

### 4.2 Match Lifecycle
1. **Pre-Game (`WAITING`):** When one seat is filled, the table waits for a second player. Once both seats are occupied, both players must click **"Ready"** (or a 5-second auto-start countdown triggers).
2. **Active Match (`IN_PROGRESS`):** The server randomly (or by host rule) assigns `Seat 1` or `Seat 2` as the initial `Active Turn Player`.
3. **End of Match (`GAME_OVER`):** When a win/loss/draw condition is met, gameplay halts, results are displayed, and ratings are updated.
4. **Rematch Flow:** At `GAME_OVER`, both players are presented with a **"Rematch"** button. If both click "Rematch," the board resets and a new match begins with alternating starting turns. If either player leaves, the table reverts to `WAITING` (if one remains) or closes (if both leave).

### 4.3 Turn-Based Gameplay Mechanics (Model A)
* **FR-7.1 (Strict Turn Enforcement):** During the `IN_PROGRESS` state, the game client and server must strictly enforce turn ownership:
  * **Active Turn Player:** Game controls/inputs (e.g., aiming, selecting a piece, setting shot power) are enabled.
  * **Waiting Player:** Game controls/inputs are completely locked/disabled. The UI must clearly indicate *"Waiting for [Opponent Name]'s turn..."* (Note: Table Chat remains fully unlocked).
* **FR-7.2 (Turn Timer):** A visible countdown timer runs during the Active Turn Player's turn. If the timer reaches `00:00` before an action is submitted, the player forfeits their turn (or forfeits the match, depending on game rules).
* **FR-7.3 (Action Submission & Validation):**
  1. The Active Turn Player executes their move and submits a discrete `TURN_ACTION` payload (e.g., vector angle, force, piece coordinates) to the server.
  2. The server validates that the sender is the `Active Turn Player` and that the action parameters are within legal bounds.
  3. Any input received from the `Waiting Player` or a `Spectator` must be immediately rejected by the server.
* **FR-7.4 (Deterministic Resolution & Synchronization):**
  * Upon validating a `TURN_ACTION`, the server broadcasts the verified action (and/or authoritative resulting state/physics keyframes) to both players and all spectators.
  * Both clients render the resulting animation/simulation identically.
  * While the simulation/animation is playing out on screen, inputs remain locked for **both** players until the server confirms all entities have come to rest and transitions state to the next player's turn.

### 4.4 Disconnections & Forfeits
* **FR-8.1:** If an active player disconnects mid-match, the game enters a `PAUSED_DISCONNECT` state and displays a grace-period countdown (e.g., 45 seconds) to the remaining player and spectators.
* **FR-8.2:** If the disconnected player reconnects within the grace period, the server sends them the complete current board snapshot, and the match resumes.
* **FR-8.3:** If the grace period expires, or if a player explicitly clicks **"Resign / Forfeit"**, the server awards the victory to the opponent and processes rating updates normally.

---

## 5. Non-Functional Requirements

* **NFR-1 (Platform Agnosticism):** All client-server communication must utilize standard, lightweight JSON payloads over bidirectional WebSockets, allowing identical backend support for Web browsers, iOS, Android, or Desktop clients.
* **NFR-2 (Authoritative Server):** The server is the single source of truth for turn ownership, rule enforcement, win conditions, and rating calculations to prevent client-side tampering.
* **NFR-3 (Deterministic Consistency):** For physics-based turn games (e.g., billiards), physics calculations must either run server-side (broadcasting position keyframes) or use fixed-point deterministic math across all clients so that the final resting positions of objects never desynchronize between `Seat 1`, `Seat 2`, and `Spectators`.
* **NFR-4 (Latency Tolerance):** Because gameplay is turn-based rather than continuous, the system must gracefully handle client latencies up to 1,000ms without degrading gameplay fairness.
```

I have created the User Requirements Document covering the Lounge, Table Matchmaking, Direct Invites, Spectator rules, and the strict turn-based synchronization flow (where the waiting player's board controls are locked while chat remains active). 

Would you like to define the **JSON WebSocket API contract** (the specific message events and payloads) next, or break down the **Database Schema** for users, ratings, and match history?
-- Boomergames Step 2 initial PostgreSQL schema (incubation/plan.md §9.5).
-- Stores only persistent state that must survive a server restart.

CREATE EXTENSION IF NOT EXISTS citext;

-- Registered players. Guests have no row.
CREATE TABLE users (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email            citext      NOT NULL UNIQUE,
    google_sub       text        UNIQUE,
    display_name     citext      NOT NULL UNIQUE,
    name_changed_at  timestamptz,
    avatar           smallint    NOT NULL DEFAULT 0,
    ignored_ids      bigint[]    NOT NULL DEFAULT '{}',
    is_moderator     boolean     NOT NULL DEFAULT false,
    muted_until      timestamptz,
    banned_until     timestamptz,
    session_version  integer     NOT NULL DEFAULT 0,
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- One row per (player, game): Elo rating + human-win high score counters.
CREATE TABLE player_stats (
    user_id          bigint   NOT NULL REFERENCES users ON DELETE CASCADE,
    game             text     NOT NULL CHECK (game IN ('pool', 'checkers')),
    rating           integer  NOT NULL DEFAULT 1200,
    rated_games      integer  NOT NULL DEFAULT 0,
    games            integer  NOT NULL DEFAULT 0,
    wins             integer  NOT NULL DEFAULT 0,
    wins_reached_at  timestamptz,
    PRIMARY KEY (user_id, game)
);

CREATE INDEX player_stats_highscore
    ON player_stats (game, wins DESC, wins_reached_at);

-- Finished games only (humans and bots). Source for match history and anti-farming.
CREATE TABLE matches (
    id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    game                  text     NOT NULL CHECK (game IN ('pool', 'checkers')),
    player1_id            bigint   REFERENCES users ON DELETE SET NULL,
    player2_id            bigint   REFERENCES users ON DELETE SET NULL,
    bot_level             smallint,
    winner                smallint CHECK (winner IN (1, 2)),
    end_reason            text     NOT NULL,
    rated                 boolean  NOT NULL,
    counts_for_highscore  boolean  NOT NULL,
    ended_at              timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX matches_p1 ON matches (player1_id, ended_at DESC);
CREATE INDEX matches_p2 ON matches (player2_id, ended_at DESC);

-- User moderation reports with an attached chat snapshot (wiped 30 days after resolve).
CREATE TABLE reports (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    reporter_id       bigint REFERENCES users ON DELETE SET NULL,
    reported_id       bigint REFERENCES users ON DELETE SET NULL,
    reported_name     text   NOT NULL,
    reason            text   NOT NULL,
    chat_log          text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    resolved_at       timestamptz,
    resolved_by       bigint REFERENCES users ON DELETE SET NULL
);

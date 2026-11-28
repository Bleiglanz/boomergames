-- Admin telemetry: user last-login tracking, persistent activity events (logins, anonymous/guest plays),
-- and persistent site settings (maintenance mode toggle & message across restarts).

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS last_login_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_login_provider text;

CREATE TABLE IF NOT EXISTS activity_events (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_type    text        NOT NULL,
    actor_user_id bigint      REFERENCES users ON DELETE SET NULL,
    actor_label   text        NOT NULL,
    auth_provider text,
    game          text,
    details       text,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS activity_events_created_at_idx
    ON activity_events (created_at DESC);

CREATE TABLE IF NOT EXISTS site_settings (
    key        text        PRIMARY KEY,
    value      text        NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

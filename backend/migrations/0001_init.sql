-- clockopus schema (docs/SPEC.md, section 6.2). The rules live in the domain code; the database
-- enforces the important ones again as a backstop.

-- R5 needs `=` on text inside a GiST index. btree_gist is a trusted extension (PostgreSQL 13+),
-- so the database owner may create it.
CREATE EXTENSION IF NOT EXISTS btree_gist;

-- One row per user. `version` grows with every change; servers cache a user's data and reload
-- it when the version differs, which also works with several replicas.
CREATE TABLE users (
    key                   text PRIMARY KEY,
    version               bigint      NOT NULL DEFAULT 0,
    created_at            timestamptz NOT NULL DEFAULT now(),
    timezone              text,
    workday_end           text        NOT NULL DEFAULT '17:00',
    forgotten_after_hours smallint    NOT NULL DEFAULT 10 CHECK (forgotten_after_hours BETWEEN 1 AND 24),
    week_start            text        NOT NULL DEFAULT 'monday' CHECK (week_start IN ('monday', 'sunday')),
    csv_rounding          smallint    NOT NULL DEFAULT 0 CHECK (csv_rounding IN (0, 5, 15))
);

CREATE TABLE tags (
    user_key text   NOT NULL REFERENCES users ON DELETE CASCADE,
    name     text   NOT NULL,
    color    text,
    position int    NOT NULL,
    PRIMARY KEY (user_key, name)
);
CREATE UNIQUE INDEX tags_name_per_user ON tags (user_key, lower(name));

CREATE TABLE tasks (
    id           uuid PRIMARY KEY,
    user_key     text        NOT NULL REFERENCES users ON DELETE CASCADE,
    title        text        NOT NULL,
    -- Tag names in the order the user gave them.
    tags         text[]      NOT NULL DEFAULT '{}',
    link         text,
    status       text        NOT NULL CHECK (status IN ('active', 'done')),
    pinned       boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL,
    last_used_at timestamptz
);
CREATE INDEX tasks_user ON tasks (user_key);

CREATE TABLE entries (
    id           uuid PRIMARY KEY,
    task_id      uuid        NOT NULL REFERENCES tasks ON DELETE CASCADE,
    user_key     text        NOT NULL REFERENCES users ON DELETE CASCADE,
    start_at     timestamptz NOT NULL,
    end_at       timestamptz,
    source       text        NOT NULL CHECK (source IN ('timer', 'manual')),
    note         text,
    keep_running boolean     NOT NULL DEFAULT false,
    -- R6: previous values, oldest first: [{"changed_at", "reason", "start", "end"}], epoch seconds.
    history      jsonb       NOT NULL DEFAULT '[]',
    -- R4
    CHECK (end_at IS NULL OR end_at > start_at),
    -- R5: entries of one user never overlap (a running entry reaches into the future). Checked at
    -- commit, so one change may pass through intermediate states.
    CONSTRAINT entries_no_overlap EXCLUDE USING gist (
        user_key WITH =,
        tstzrange(start_at, coalesce(end_at, 'infinity')) WITH &&
    ) DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX entries_user_start ON entries (user_key, start_at);
-- R1: at most one running entry per user.
CREATE UNIQUE INDEX entries_one_running ON entries (user_key) WHERE end_at IS NULL;

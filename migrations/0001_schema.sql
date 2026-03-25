-- App dictionary
CREATE TABLE IF NOT EXISTS apps (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

-- Device dictionary
CREATE TABLE IF NOT EXISTS devices (
    id          INTEGER PRIMARY KEY,
    external_id TEXT UNIQUE,
    model       TEXT NOT NULL DEFAULT ''
);

-- Usage fact table
CREATE TABLE IF NOT EXISTS usage (
    event_id    INTEGER PRIMARY KEY,
    app_id      INTEGER NOT NULL REFERENCES apps(id),
    amount      INTEGER NOT NULL,
    start_time  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    tz_offset   INTEGER NOT NULL,
    device_id   INTEGER NULL REFERENCES devices(id)
);

-- Typical export/sync query indexes
CREATE INDEX IF NOT EXISTS idx_usage_end_expr
ON usage((start_time + amount));

CREATE INDEX IF NOT EXISTS idx_usage_app_end_expr
ON usage(app_id, (start_time + amount));

CREATE INDEX IF NOT EXISTS idx_usage_app
ON usage(app_id);

-- screenhistory: initial schema (sqlx migration 0001)
-- Mirrors current runtime-created schema in db.rs

-- Main usage table. Epoch times stored as REAL (seconds) to match existing code.
CREATE TABLE IF NOT EXISTS usage (
    event_id     INTEGER PRIMARY KEY,  -- ZOBJECT.Z_PK from Apple's DB
    app_name     TEXT NOT NULL,        -- ZOBJECT.ZVALUESTRING
    amount       REAL NOT NULL,        -- duration seconds
    start_time   REAL NOT NULL,        -- epoch seconds
    end_time     REAL NOT NULL,        -- epoch seconds
    created_at   REAL NOT NULL,        -- epoch seconds
    tz_offset    INTEGER NOT NULL,     -- seconds from GMT
    device_id    TEXT NULL,            -- optional device identifier
    device_model TEXT NOT NULL         -- model string (empty if unknown)
);

-- Indexes for typical queries and exports
CREATE INDEX IF NOT EXISTS idx_usage_end_time ON usage(end_time);
CREATE INDEX IF NOT EXISTS idx_usage_app_name ON usage(app_name);

-- Generic key-value metadata table for future use
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

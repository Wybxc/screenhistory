-- Optimize storage: normalize apps/devices, switch REAL→INTEGER, drop stored end_time/app_name.
-- This migration preserves the existing public interface by replacing `usage` table
-- with an updatable VIEW + INSTEAD OF INSERT trigger, so existing code keeps working.

-- 1) Lookup tables
CREATE TABLE IF NOT EXISTS apps (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS devices (
    id          INTEGER PRIMARY KEY,
    external_id TEXT UNIQUE,         -- may be NULL for unknown
    model       TEXT NOT NULL DEFAULT ''
);

-- 2) Optimized fact table (INTEGER seconds, normalized FKs, no stored end_time, no app_name, no device_model)
CREATE TABLE IF NOT EXISTS usage_v2 (
    event_id    INTEGER PRIMARY KEY,
    app_id      INTEGER NOT NULL REFERENCES apps(id),
    amount      INTEGER NOT NULL,    -- seconds
    start_time  INTEGER NOT NULL,    -- epoch seconds (UTC)
    created_at  INTEGER NOT NULL,    -- epoch seconds (UTC)
    tz_offset   INTEGER NOT NULL,    -- seconds from GMT
    device_id   INTEGER NULL REFERENCES devices(id)
);

-- 3) Backfill lookup tables from legacy `usage` (if present)
--    Populate apps
INSERT OR IGNORE INTO apps(name)
SELECT DISTINCT app_name
FROM usage
WHERE app_name IS NOT NULL;

--    Populate devices (only when a non-NULL external id exists)
--    Choose an arbitrary non-empty model when multiple exist.
INSERT OR IGNORE INTO devices(external_id, model)
SELECT device_id,
       COALESCE(NULLIF(MAX(COALESCE(device_model, '')), ''), '')
FROM usage
WHERE device_id IS NOT NULL
GROUP BY device_id;

-- 4) Backfill usage_v2 from legacy `usage`
INSERT OR IGNORE INTO usage_v2(event_id, app_id, amount, start_time, created_at, tz_offset, device_id)
SELECT
    u.event_id,
    a.id                                 AS app_id,
    CAST(u.amount AS INTEGER)            AS amount,
    CAST(u.start_time AS INTEGER)        AS start_time,
    CAST(u.created_at AS INTEGER)        AS created_at,
    u.tz_offset                          AS tz_offset,
    d.id                                 AS device_id
FROM usage AS u
JOIN apps  AS a ON a.name = u.app_name
LEFT JOIN devices AS d ON d.external_id IS u.device_id OR d.external_id = u.device_id;

-- 5) Drop legacy indexes to reclaim space (they will be unused after we replace the table)
DROP INDEX IF EXISTS idx_usage_end_time;
DROP INDEX IF EXISTS idx_usage_app_name;

-- 6) Replace `usage` table with a compatibility VIEW exposing the old shape.
--    First, rename away the original table.
ALTER TABLE usage RENAME TO usage_legacy;

--    Create view `usage` that computes end_time and joins to names/models.
CREATE VIEW usage AS
SELECT
    v.event_id                                   AS event_id,
    a.name                                       AS app_name,
    v.amount                                     AS amount,
    v.start_time                                 AS start_time,
    (v.start_time + v.amount)                    AS end_time,
    v.created_at                                 AS created_at,
    v.tz_offset                                  AS tz_offset,
    d.external_id                                AS device_id,
    COALESCE(d.model, '')                        AS device_model
FROM usage_v2 AS v
JOIN apps AS a
  ON a.id = v.app_id
LEFT JOIN devices AS d
  ON d.id = v.device_id;

-- 7) Make the view updatable for INSERTs used by ingestion code.
--    We accept all old columns, but ignore the provided end_time and compute it.
CREATE TRIGGER IF NOT EXISTS usage_ioi_insert
INSTEAD OF INSERT ON usage
BEGIN
    -- Ensure app exists
    INSERT OR IGNORE INTO apps(name) VALUES (NEW.app_name);

    -- Ensure device exists when provided; set/keep a model string
    INSERT OR IGNORE INTO devices(external_id, model)
    SELECT NEW.device_id, COALESCE(NEW.device_model, '')
    WHERE NEW.device_id IS NOT NULL;

    UPDATE devices
       SET model = COALESCE(NULLIF(NEW.device_model, ''), model)
     WHERE external_id IS NEW.device_id;

    -- Insert the fact row (INTEGER seconds); ignore duplicates by event_id
    INSERT OR IGNORE INTO usage_v2(
        event_id, app_id, amount, start_time, created_at, tz_offset, device_id
    )
    VALUES (
        NEW.event_id,
        (SELECT id FROM apps WHERE name IS NEW.app_name OR name = NEW.app_name),
        CAST(NEW.amount AS INTEGER),
        CAST(NEW.start_time AS INTEGER),
        CAST(NEW.created_at AS INTEGER),
        NEW.tz_offset,
        (SELECT id FROM devices WHERE external_id IS NEW.device_id OR external_id = NEW.device_id)
    );
END;

-- 8) Indexes targeting typical queries (by time range and/or app)
--    Index on computed end_time expression
CREATE INDEX IF NOT EXISTS idx_usage_v2_end_expr
ON usage_v2((start_time + amount));

--    Composite: app then end_time expression, for app-scoped time queries
CREATE INDEX IF NOT EXISTS idx_usage_v2_app_end_expr
ON usage_v2(app_id, (start_time + amount));

--    App-only filter
CREATE INDEX IF NOT EXISTS idx_usage_v2_app
ON usage_v2(app_id);

-- 9) Legacy cleanup: drop the old table to reclaim space (safe after backfill)
DROP TABLE IF EXISTS usage_legacy;

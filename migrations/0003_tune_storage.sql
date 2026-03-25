-- screenhistory: tune storage to reduce file size and reclaim space
--
-- Notes:
-- - Changing page_size and auto_vacuum settings takes effect after VACUUM.
-- - VACUUM cannot run inside a transaction. This migration is intended to run
--   without a surrounding transaction by the migration runner.
-- - wal_checkpoint(TRUNCATE) shrinks the WAL file immediately.
--
-- Safe to re-run: PRAGMAs are idempotent and VACUUM is harmless.

-- Prefer smaller pages for small/local DBs to reduce overhead.
PRAGMA page_size = 1024;

-- Reclaim space automatically as pages become free.
PRAGMA auto_vacuum = FULL;





-- Optional: run planner optimizations and clean internal caches/statistics.
PRAGMA optimize;

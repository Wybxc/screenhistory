# screenhistory

Store a permanent local history of your macOS Screen Time data, view or export it on demand, and keep it updated automatically via a launchd job that invokes the CLI.

Goals:
- Persist Screen Time beyond Apple’s ~7-day window
- Keep it small and unattended (launchd runs the CLI; it exits when done)
- Allow ad-hoc actions: sync now, export CSV/JSON
- Avoid a menubar app for now

## What’s included

- crates/core: shared Rust library
  - Local SQLite schema and migrations
  - Incremental ingest from Apple’s Screen Time DB (knowledgeC.db)
  - Export to CSV/JSON
- crates/cli: headless CLI
  - `sync` to ingest new events
  - `export` with filters
- packaging/launchd: LaunchAgent plist template

Local database: `~/.screenhistory.sqlite`

Screen Time (source) DB: `~/Library/Application Support/Knowledge/knowledgeC.db`

Note: Reading Apple’s DB requires Full Disk Access (FDA). Grant FDA to the screenhistory CLI binary when syncing.

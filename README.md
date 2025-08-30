# screenhistory

Persist your macOS Screen Time history locally and export it on demand. A launchd agent keeps it updated.

Notes:
- Requires Full Disk Access (FDA) for the installed binary.
- Reads Apple’s `~/Library/Application Support/Knowledge/knowledgeC.db`. If Apple removes it, this stops working.

## Quick start
1) Install: `just install` (installs to `/usr/local/bin/screenhistory`)
2) Grant FDA to `/usr/local/bin/screenhistory`  
   System Settings → Privacy & Security → Full Disk Access → + → `/usr/local/bin/screenhistory`
3) Schedule hourly sync: `screenhistory schedule install`
4) Check logs: `tail -n 50 ~/Library/Logs/screenhistory/sync.log`

## Install
- With just:
  - `just install` (build + install)
  - `just release` (build only, binary at `target/release/screenhistory`)
- With cargo:
  - `cargo install --path crates/cli`

## Scheduling (launchd)
- Label: `com.mikkelam.screenhistory`
- Plist: `~/Library/LaunchAgents/com.mikkelam.screenhistory.plist`
- Logs: `~/Library/Logs/screenhistory/sync.log`

Common:
- Hourly (default): `screenhistory schedule install`
- Every N: `screenhistory schedule install --every 15m`
- Daily at time: `screenhistory schedule install --at 02:00`
- Run now / status / uninstall:
  - `screenhistory schedule run-now`
  - `screenhistory schedule status`
  - `screenhistory schedule uninstall`

Notes:
- `--every` and `--at` are mutually exclusive.
- `--run-at-load` runs once immediately after install.

## Usage
- Sync now: `screenhistory sync`
- Export CSV: `screenhistory export --format csv --out usage.csv`
- Export JSON: `screenhistory export --format json`
- Filters: `--from 2025-01-01 --to 2025-01-31 --app "Safari"`

## Paths
- Local DB: `~/.screenhistory.sqlite`
- Source DB: `~/Library/Application Support/Knowledge/knowledgeC.db`
- Logs: `~/Library/Logs/screenhistory/sync.log`

## Troubleshooting
- Status: `launchctl print gui/$(id -u)/com.mikkelam.screenhistory`
- Kick/run: `launchctl kickstart -k gui/$(id -u)/com.mikkelam.screenhistory`
- Logs: `tail -n 100 ~/Library/Logs/screenhistory/sync.log`
- “Operation not permitted” → re-check FDA for `/usr/local/bin/screenhistory`

## Include other Apple devices
Enable “Share Across Devices” in Screen Time on all devices. It can take time before data appears locally.
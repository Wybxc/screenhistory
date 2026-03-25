# screenhistory

Persist your macOS Screen Time history locally and export it on demand. A launchd agent keeps it updated.

## Notes

- Requires Full Disk Access (FDA) for the installed binary
- Reads Apple's `~/Library/Application Support/Knowledge/knowledgeC.db`
- If Apple removes this database, this tool will stop working

## Quick start

1. Install: `cargo install --path .` (installs to `~/.cargo/bin/screenhistory`)
2. Grant Full Disk Access to the binary:
   - System Settings → Privacy & Security → Full Disk Access → + → `~/.cargo/bin/screenhistory`
3. Schedule hourly sync: `screenhistory schedule install`
4. Check logs: `tail -n 50 ~/Library/Logs/screenhistory/sync.log`

## Install

### With cargo

```bash
cargo install --path .
```

The binary will be installed to `~/.cargo/bin/screenhistory`.

### With Homebrew (optional)

If you have a custom Homebrew tap set up:

```bash
brew tap Wybxc/screenhistory
brew install screenhistory
```

## Scheduling (launchd)

### Configuration

- Label: `com.mikkelam.screenhistory`
- Plist: `~/Library/LaunchAgents/com.mikkelam.screenhistory.plist`
- Logs: `~/Library/Logs/screenhistory/sync.log`

### Installation and management

- Install with hourly sync (default): `screenhistory schedule install`
- Install with custom interval: `screenhistory schedule install --every 15m`
- Install with daily time: `screenhistory schedule install --at 02:00`
- Run immediately: `screenhistory schedule run-now`
- Check status: `screenhistory schedule status`
- Uninstall: `screenhistory schedule uninstall`

### Schedule options

- `--every <interval>`: Set interval like "15m", "1h", or seconds (e.g., "900"). Mutually exclusive with `--at`.
- `--at <time>`: Run daily at 24h time (HH:MM format). Mutually exclusive with `--every`.
- `--run-at-load`: Also run immediately after installation.
- `--local-db <path>`: Path to local history DB (forwarded to sync).
- `--knowledge-db <path>`: Path to macOS Screen Time DB (forwarded to sync).
- `--log-file <path>`: Log file path (defaults to `~/Library/Logs/screenhistory/sync.log`).

## Usage

### Sync

Sync from macOS Screen Time database to local history:

```bash
screenhistory sync
screenhistory sync --verbose
screenhistory sync --local-db ~/custom.db --knowledge-db ~/custom-knowledge.db
```

### Export

Export usage history to CSV or JSON:

```bash
# Export to CSV (default)
screenhistory export --format csv --out usage.csv

# Export to JSON
screenhistory export --format json --out usage.json

# Export to stdout
screenhistory export --format csv

# With filters
screenhistory export --from 2025-01-01 --to 2025-01-31 --app "Safari"
```

#### Export options

- `--local-db <path>`: Path to local history DB (defaults to `~/.screenhistory.sqlite`)
- `--from <datetime>`: Filter by start date (RFC3339 like `2025-01-02T03:04:05Z`, or date-only `2025-01-02`)
- `--to <datetime>`: Filter by end date (RFC3339 like `2025-01-02T03:04:05Z`, or date-only `2025-01-02`)
- `--app <name>`: Filter to export only rows for a specific app name
- `--format <format>`: Output format: `csv` or `json` (default: `csv`)
- `--out <path>`: Output file path (defaults to stdout if omitted)

## Development

### Sanitized Test Fixture

To keep runtime logic and development/test tooling separate, fixture generation is provided as a standalone script:

```bash
python3 scripts/make_sanitized_fixture.py \
  --source knowledgeC.db \
  --out tests/fixtures/knowledgeC.sanitized.db \
  --max-rows 1500
```

This generates a sanitized fixture DB that strips personally identifiable information while preserving the schema and query patterns. The integration test `tests/sanitized_fixture_sync.rs` consumes this fixture.

### Fixture notes

- If you see `Operation not permitted`, grant Full Disk Access to your terminal (or the executable running this command)
- Fixture generation is intended as a one-time action; commit the generated fixture file for repeatable tests

## Paths

- **Local DB**: `~/.screenhistory.sqlite` (stores synced history)
- **Source DB**: `~/Library/Application Support/Knowledge/knowledgeC.db` (Apple's Screen Time database)
- **Logs**: `~/Library/Logs/screenhistory/sync.log` (launchd sync logs)
- **LaunchAgent**: `~/Library/LaunchAgents/com.mikkelam.screenhistory.plist`

## Troubleshooting

### LaunchAgent issues

Check status:

```bash
launchctl print gui/$(id -u)/com.mikkelam.screenhistory
```

Kick/run immediately:

```bash
launchctl kickstart -k gui/$(id -u)/com.mikkelam.screenhistory
```

View logs:

```bash
tail -n 100 ~/Library/Logs/screenhistory/sync.log
```

### Permission issues

If you see "Operation not permitted":

1. Re-check Full Disk Access for `/usr/local/bin/screenhistory` (or `~/.cargo/bin/screenhistory`)
2. System Settings → Privacy & Security → Full Disk Access
3. Ensure the correct binary path is granted access

## Multi-device sync

Enable "Share Across Devices" in Screen Time on all devices. It can take time before data appears locally.

## License

Licensed under the [MIT License](LICENSE).

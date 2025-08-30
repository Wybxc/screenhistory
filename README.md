# screenhistory

Store a permanent local history of your macOS Screen Time data, view or export it on demand, and keep it updated automatically via a launchd job that invokes the CLI.

Features:
- Persist Screen Time beyond Apple’s ~7-day window
- Keep it small and unattended (launchd runs the CLI; it exits when done)
- Allow ad-hoc actions: sync now, export CSV/JSON
- Avoid a menubar app for now

This CLI completely relies on apple's `~/Library/Application Support/Knowledge/knowledgeC.db`. If apple removes this, the CLI will stop working.

## Usage

```console
❯ screenhistory --help
Headless CLI to sync/export Screen Time history

Usage: screenhistory <COMMAND>

Commands:
  sync      Sync from macOS Screen Time DB into the local history DB
  export    Export local history to CSV or JSON
  schedule  Manage launchd scheduling for periodic sync
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## Installation

This project uses just. To install to `/user/local/bin/` run `just install`

Note: Reading Apple’s DB requires Full Disk Access (FDA). Grant FDA to the screenhistory CLI binary to successfully sync!

## Include other apple devices screen time

Open screen time on all devices you wish to sync and check: `Share across devices`. Note that it can take a while for apple to start syncing this data to the internal database. Screenhistory will grab all available data from the internal database.

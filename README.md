# screenhistory

Store a permanent local history of your macOS Screen Time data, view or export it on demand, and keep it updated automatically via a launchd job that invokes the CLI.

Features:
- Persist Screen Time beyond Apple’s ~7-day window
- Keep it small and unattended (launchd runs the CLI; it exits when done)
- Allow ad-hoc actions: sync now, export CSV/JSON
- Avoid a menubar app for now

## Installation

This project uses just. To install to `/user/local/bin/` run `just install`

Note: Reading Apple’s DB requires Full Disk Access (FDA). Grant FDA to the screenhistory CLI binary to successfully sync!

## Include other apple devices screen time

Open screen time on all devices you wish to sync and check: `Share across devices`. Note that it can take a while for apple to start syncing this data to the internal database. Screenhistory will grab all available data from the internal database.

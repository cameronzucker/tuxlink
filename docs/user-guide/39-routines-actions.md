# Routines actions reference

A routine is a saved sequence of steps that the app runs for you. Each step
calls one **action**. This page is the catalog of the actions a routine can
use: what each one reads or does, the parameters it takes, and what it hands
back to the next step. It also explains the two consent classes, transmitting
and writing, in plain terms, so you know which routines can run on their own
and which stop to ask you first.

Actions are grouped by name. A name like `data.read` or `config.set_ardop` is
the identifier you pick in the step editor; the label in parentheses is what
shows on the tile.

## Reading station state (data.read)

`data.read` (Read station state) is the workhorse read action. It takes one
parameter, `source`, and returns a snapshot of that part of the station. It
never touches the radio, never reaches the internet, and never changes
anything. If a later step branches on what the station looks like, this is how
it gets the picture.

There are thirteen sources. Each returns the same curated, non-secret view an
AI assistant would see through the app's agent tools, so credentials and
over-precise location are already trimmed out.

| `source` | What it returns |
|---|---|
| `grid` | Your Maidenhead grid square, for example `AA00aa`. |
| `inbox_summary` | Message counts: total and unread. |
| `space_weather` | The latest space-weather snapshot (solar flux, K-index, and so on), or nothing when none has been fetched. |
| `heard_stations` | Currently unavailable. The app does not keep a queryable record of heard APRS stations, so this source returns an honest "not available" message rather than empty data. |
| `last_connected_gateway` | The last radio gateway you connected to, or a "not yet" message until a Packet, ARDOP, or VARA session has completed. |
| `modem_status` | The curated modem status: which modem is selected, whether it is connected, and its state. |
| `backend_status` | The CMS engine (Winlink backend) status: connected or not, transport, and state, with login details redacted. |
| `app_status` | The app's own view: name, version, whether send-authority is armed and for how long, and whether the session is tainted. |
| `config` | The non-secret top-level station config: whether CMS is enabled, the transport, host, callsign, and a 4-character grid. |
| `ardop_config` | The ARDOP modem settings: host, port, drive level, bandwidth. |
| `vara_config` | The VARA modem settings: host, port, bandwidth, drive level. |
| `packet_config` | The packet (AX.25 / KISS) settings: KISS host and port, baud, TX delay. |
| `rig_config` | The rig (CAT control) settings: hamlib model, rigctld host and port, serial path and baud, and the sequencing options. |

Example step:

```json
{ "action": "data.read", "params": { "source": "modem_status" } }
```

## Actions that do work

Beyond reading, three actions in this release find gateways, search the app's
own help, and write a single config value. Each is described with its
parameters and a small example.

### data.find_stations (Find gateway stations)

Looks up Winlink gateway stations from the public directory and returns them
sorted by distance from your grid. This is the action that feeds a connect
step: its `callsigns` output is a plain list you can hand straight to
`radio.connect`. It reaches the internet to poll the directory; it never
transmits.

Parameters, all optional:

- `modes` - a list of listing modes to include, using the same names as the
  station-list refresh, for example `vara-hf`, `ardop`, `packet`. Leave it out
  to include every confirmed mode.
- `bands` - a list of bands to keep, for example `20m`, `40m`. Leave it out to
  keep all bands.
- `history_hours` - how far back the directory's activity window reaches. Must
  be 720 or less (30 days); a larger value is rejected.
- `limit` - the most stations to return, counted by distinct callsign. A value
  of 0 is rejected as a nonsense request.

Output: an object with `gateways` (the surviving directory rows), `callsigns`
(the de-duplicated callsign list, in distance order, truncated to `limit`),
`fetched_at_ms` (when the directory was last fetched), and `operator_grid`
(the 4-character grid used for the distance sort).

Example step:

```json
{ "action": "data.find_stations", "params": { "modes": ["vara-hf"], "limit": 3 } }
```

A following `radio.connect` step reads `$s1.callsigns` (the output of the step
named `s1`) to try those stations in order.

### data.docs_search (Search app docs)

Searches this app's own documentation, the same corpus behind the in-app help
search, and returns the matching pages. It is entirely local: no internet, no
radio, no writes. Use it when a routine needs to point you at a help topic.

Parameter:

- `query` - the search text. It must not be empty; a blank query is rejected
  as an authoring mistake.

Output: an object with `hits`, each carrying the page `title`, its `slug`, and
a short `snippet`. No matches returns an empty `hits` list, not an error.

Example step:

```json
{ "action": "data.docs_search", "params": { "query": "find stations" } }
```

### config.set_ardop (Set ARDOP config)

Sets the ARDOP transmit drive level (0 to 100) in your station config. This is
the first action that **writes** configuration, so it belongs to the writing
consent class described below. Writing config does not key the radio; it only
changes a stored setting.

Parameter:

- `drive_level` - a number from 0 to 100. A value above 100 is rejected before
  anything is read or written.

Output: an object naming the `field` changed (`drive_level`), the `old` value
(or nothing if none was set), and the `new` value. The write is done under the
same lock the app's other config writers use, so it cannot race another change
or erase neighboring settings.

Example step:

```json
{ "action": "config.set_ardop", "params": { "drive_level": 80 } }
```

## Consent classes: what stops to ask you

Most actions read state or queue work and run without interruption. Two kinds
of action are different because they change the world outside the app, and both
are gated by your consent.

### Transmitting routines

Any step that keys the radio (a connect, a message exchange, a tune) is a
**transmitting** step. Transmitting is a Part 97 control-operator act, so the
operator has to be in the loop.

- **Attended.** The routine stops at each transmitting step and shows a
  confirmation. You click to transmit, or cancel the run. Nothing goes on the
  air without that click.
- **Automatic (unattended).** A routine you have set to run on its own cannot
  stop to ask, so it may only transmit if you have given it the transmit
  **acknowledgment** ahead of time. Without that acknowledgment, an automatic
  routine with a transmitting step will not run.

### Writing routines

Any step that changes station configuration (the `config.*` actions, such as
`config.set_ardop`) is a **writing** step. A config write does not key the
radio, so it is treated as its own, separate class with its own confirmation
and its own acknowledgment.

- **Attended.** The routine stops at each write and shows a "Confirm config
  write" prompt in plain, non-transmit language. You confirm the change, or
  cancel the run.
- **Automatic (unattended).** An automatic routine that writes config may only
  do so if you have given it the write **acknowledgment** ahead of time.
  Without it, the routine will not run automatically.

### How an acknowledgment binds

An acknowledgment is not a blanket permission. It is signed against the exact
set of steps it covers, captured as a fingerprint at the moment you
acknowledge. That fingerprint includes every step the routine runs, including
steps in any routine it calls.

If you later change the routine, or change a routine it calls, the fingerprint
no longer matches and the acknowledgment is no longer valid. The app tells you
so and asks you to re-acknowledge before the routine will run automatically
again. This is deliberate: it guarantees that what you approved is exactly what
runs, and that a change slipped into a called routine cannot ride an old
approval onto the air or into your config.

The transmit and write acknowledgments are independent. A routine that both
transmits and writes needs both, and each is invalidated on its own if the
covered steps change.

## Triggers: when a routine runs

Every routine carries a `triggers` list in its definition. There are two
kinds:

- **Manual** (`{"type": "manual"}`): the routine runs only when you press Run,
  or when an agent requests a run on your behalf. No other fields.
- **Schedule**: the routine fires itself on an interval.

A schedule trigger looks like this:

```json
{ "type": "schedule", "every": "1h", "align": "hour" }
```

- `every` is the interval, written like `"30m"`, `"2h"`, or `"45s"`. Required.
- `align` is optional: `"hour"` or `"day"` snaps firing to the top of the hour
  or day. `"every": "1h"` with `"align": "hour"` means 00:00, 01:00, 02:00,
  and so on.
- `window` is optional: a local-time window like `"08:00-20:00"`. Outside the
  window the schedule stays quiet.
- `if_missed` is optional: `"skip"` (the default) ignores fires the app was
  closed for; `"run_once_on_launch"` runs one catch-up when the app comes
  back.

A scheduled routine still honors the consent classes above. In attended mode,
each scheduled fire parks at the transmitting (or config-writing) step and
waits for your confirmation, so schedule plus attended only makes sense when
you expect to be at the station. For unattended operation the routine must be
automatic, which requires your recorded acknowledgment first.

## Where next

- [AI agent integration (MCP)](35-agent-mcp.md) - the send-authority and taint
  model the read sources mirror.
- [Operating modes](33-operating-modes.md) - what a `radio.connect` step is
  actually opening.
- [Position and privacy](26-position-and-privacy.md) - why the grid and config
  reads are clamped to 4 characters.

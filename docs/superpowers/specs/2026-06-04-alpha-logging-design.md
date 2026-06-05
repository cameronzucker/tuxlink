# Alpha-logging — design

> **Date:** 2026-06-04 · **Author:** `sequoia-pika-tamarack` (brainstorm with operator) · **Status:** DRAFT
>
> **Scope:** Robust + compact + portable diagnostic logging for the tuxlink desktop app, with a "Compress and export logs" menu item that produces a single shareable archive for alpha-tester bug reports. Greenfield infrastructure — no existing `tracing` / `tauri-plugin-log` / `env_logger` in `src-tauri/Cargo.toml`. Forward-applicable beyond alpha: addresses recurring WLE pain documented in [dev/research/2026-06-04-winlink-group-pain-points.md](../../../dev/research/2026-06-04-winlink-group-pain-points.md) (transport failures 20.7%, audio fragility 12.1%, password class 15.1%).
>
> **bd:** to be filed as umbrella + child issues post-design via `superpowers:writing-plans`. Big-bang single-PR shape per operator direction.
>
> **Alpha framing:** per memory `alpha-is-vettedness-not-built-ness`, this feature ships fully built or not at all. Per operator direction this session, environment-probe coverage is a **hard alpha-candidate requirement**, not enhancement scope. Per memory `inline-ui-no-window-clutter`, exception granted for the Logging window — infrequent admin surface, mirrors the existing `help_window.rs` pattern.

---

## 1. Goals and constraints

### Goals (forcing functions, not options)

1. **Robust.** Captures enough action history to reconstruct what an alpha tester did before a bug surfaced. Per-target verbosity matrix biased toward where alpha-bug-report pain comes from (transport / modem / B2F at higher detail; UI / mailbox / forms at lower).
2. **Compact.** Bug-report artifacts may travel over SMS, Winlink message attachments, or Discord paste. Default-install exports target single-digit MB (with the bundled dictionary); summary.txt is paste-friendly under 500 bytes for cases where only a headline can transit.
3. **Portable.** UTC timestamps, correlation IDs, no machine-specific paths in metadata. Archive decompresses with stock `tar` + `zstd` — no tuxlink-specific tools required at the agent's end.
4. **"Compress and export logs" menu item.** Real UI surface in the Logging window. Single-click action produces one shareable archive.
5. **Forward-applicable.** Design serves the post-alpha product. WLE corpus pain patterns are the validation case.

### Non-goals (explicit deferrals — see §10)

- Real-corpus-trained zstd dictionary — v1 (asset swap)
- Allowlist-based redaction promotion — beta or later
- Per-subsystem verbosity sliders — post-alpha
- Dedicated in-app log viewer with filter/query — separate UI work; the existing radio-panel session-log strip handles live-tail
- `gh` CLI detection / GitHub PAT integration — post-alpha if URL-pre-fill friction surfaces

---

## 2. Architecture

### 2.1 Stream model

**One `tracing` stream, two renderings.** Emission sites use `tracing` macros (`info!`, `debug!`, `warn!`, `error!`); the subscriber composition routes the same events to both the UI's existing `SessionLogState` ring buffer (no schema change to UI) and to disk as JSONL.

This unifies the existing `src-tauri/src/session_log.rs` ring buffer with the new diagnostic-log infrastructure. The UI's radio-panel session-log strip continues to receive its existing `LogLine` shape via a thin adapter layer that consumes tracing events and calls `SessionLogState::append()`.

### 2.2 Pipeline

```
                ┌─────────────────────────────────────────┐
                │  Emission callsites (tracing::info!, …) │
                │  ~70 src-tauri modules                  │
                └────────────────┬────────────────────────┘
                                 │
                  ┌──────────────▼───────────────┐
                  │ tuxlink::logging::Subscriber │  (composes layers)
                  └──────────────┬───────────────┘
                                 │
            ┌────────────────────┼────────────────────┬─────────────────────┐
            ▼                    ▼                    ▼                     ▼
     ┌────────────┐      ┌────────────┐      ┌─────────────┐       ┌───────────────┐
     │ Redaction  │      │ UI Layer   │      │ Disk Layer  │       │ Filter Layer  │
     │ (per-field │      │ (forwards  │      │ (rolling    │       │ (per-target   │
     │  scrubber) │      │  to        │      │  JSONL via  │       │  level rules; │
     │            │      │  Session   │      │  tracing-   │       │  detailed-    │
     │            │      │  LogState) │      │  appender)  │       │  mode toggle) │
     └────────────┘      └─────┬──────┘      └──────┬──────┘       └───────────────┘
                               │                    │
                               ▼                    ▼
                        Existing radio       $XDG_STATE_HOME/
                        panel session-log    tuxlink/logs/
                        strip (no change)    tuxlink.YYYY-MM-DD-HH.jsonl
                                                      │
                                                      ▼
                                            ┌──────────────────────┐
                                            │ Export builder       │
                                            │ - read all retained  │
                                            │ - render summary.txt │
                                            │ - render manifest    │
                                            │ - inner: zstd+dict   │
                                            │ - outer: tar.zst     │
                                            └──────────┬───────────┘
                                                       ▼
                                            tuxlink-logs-{ts}-{corr-id}.tar.zst
```

### 2.3 Crates

| Crate | Version | Role |
|---|---|---|
| `tracing` | `^0.1` | Emission macros, span API |
| `tracing-subscriber` | `^0.3` | Subscriber + Layer composition |
| `tracing-appender` | `^0.2` | Non-blocking rolling-file appender |
| `zstd` | `^0.13` (with `zdict` feature) | Compression + dictionary support |
| `tar` | `^0.4` | Outer-archive packaging |
| `dirs` | `^5` | `state_dir()` lookup for `$XDG_STATE_HOME` |
| `regex` | `^1.10` (already present via other deps) | Redaction blocklist |
| `uuid` | already at `^1.23` | Boot ID (UUID v7) |

### 2.4 Module breakdown

**New `src-tauri/src/logging/` module:**

```
src-tauri/src/logging/
├── mod.rs                   Public init() + Tauri command handlers
├── subscriber.rs            Subscriber composition
├── redact.rs                Field blocklist regex + Visitor implementation
├── ui_layer.rs              Layer that forwards into SessionLogState
├── disk_layer.rs            Wraps tracing-appender rolling output
├── filter_layer.rs          Per-target level rules + Detailed-mode toggle
├── retention.rs             Sweep logic (days + size caps)
├── free_disk_guard.rs       5-minute poll + warn-event when filesystem tight
├── export.rs                Build outer tar.zst; inner events.jsonl.zst + dict
├── dict.rs                  Embeds .zdict via include_bytes!; exposes for export
├── manifest.rs              Build / OS / policy-version metadata renderer
├── summary.rs               Renders summary.txt from event tail
├── settings.rs              Persisted Detailed-mode + retention state
└── env_probes/
    ├── mod.rs               Probe trait + dispatch
    ├── keyring.rs           Secret Service / D-Bus / collection state
    ├── audio.rs             ALSA / PipeWire / device-list state
    ├── serial.rs            /dev/serial/by-id listing + permissions
    ├── modem_process.rs     VARA / ARDOP process state
    ├── network.rs           DNS / route / CMS reachability
    └── display.rs           Wayland / X11 / WebKit / GPU state
```

**New `src-tauri/src/logging_window.rs`:** mirrors [`src-tauri/src/help_window.rs`](../../../src-tauri/src/help_window.rs) shape exactly (single-instance Tauri webview, main-window-only invoker guard, idempotent focus on re-invoke, `WindowLabelAlreadyExists` race-guard).

**New `xtask` crate at repo root:**

```
xtask/
├── Cargo.toml
├── README.md                Documents both binaries
└── src/bin/
    ├── gen-corpus.rs        Synthetic event-corpus generator
    └── train-log-dict.rs    zstd::dict::from_files() driver
```

**New asset:**

```
src-tauri/assets/logging/
└── tuxlink-events-v1.zdict  ~16 KB, synthetic-corpus-trained
```

**New frontend route:** `/logging` rendering `src/help/LoggingView.tsx`.

**Touched files:**
- `src-tauri/src/lib.rs` — wire `logging::init()` at startup, register Tauri commands, register `logging_window_open`
- `src-tauri/src/main.rs` — call `logging::init()` before Tauri builder starts
- `src-tauri/Cargo.toml` — add tracing + zstd + tar deps
- `src/shell/chrome/menuModel.ts` — add `menu:help:logging`, keep `menu:help:report_issue` id (behavior changes)
- `src/shell/chrome/dispatchMenuAction.ts` — route the two help actions to the new commands
- Every src-tauri module that the per-target matrix (§4.1) names — add `tracing` imports + emission calls at the documented sites (see §4.4 enumeration)

### 2.5 Reuse of `session_log.rs`

The existing `SessionLogState` ring buffer (`src-tauri/src/session_log.rs`) is unchanged. The new `logging::ui_layer` is a `tracing_subscriber::Layer` impl whose `on_event` callback constructs a `LogLine` from the tracing event and calls `SessionLogState::append()`. The radio-panel session-log strip continues to read from `SessionLogState` via `session_log_snapshot` and the existing broadcast channel — no React-side changes.

The `seq` field on disk-layer-emitted JSONL events comes from the same `SessionLogState::next_seq` counter, so UI panel events and disk events share the same monotonic identifier space (useful for cross-correlating "what the operator saw on screen" with "what landed in the bug-report archive").

---

## 3. Data model

### 3.1 Event schema (one JSONL line per event, on disk and in the archive)

```json
{
  "v": 1,
  "ts": "2026-06-04T12:34:56.789012Z",
  "boot": "01927a8b-9c12-7000-a4d3-2f8e1b9c0001",
  "seq": 42891,
  "level": "info",
  "target": "tuxlink::winlink::session",
  "span": { "name": "dial_attempt", "id": "0x7f3a", "attempt_id": "att-xyz1" },
  "msg": "dial start",
  "fields": {
    "transport": "vara",
    "gateway": "K6XXX-10",
    "frequency_hz": 7104000,
    "callsign": "K0ABC"
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `v` | integer | Schema version. `1` for first release. Bumped on breaking change. |
| `ts` | string (RFC3339) | UTC, microsecond precision. |
| `boot` | string (UUID v7) | Minted at process start in `logging::init()`. Lives on the `Subscriber`. Unique per process launch. |
| `seq` | integer | Monotonic; shared with `SessionLogState::next_seq`. |
| `level` | string enum | `trace` \| `debug` \| `info` \| `warn` \| `error`. |
| `target` | string | Tracing target string (typically module path). |
| `span` | object \| absent | Present when event is inside a tracing span. See §3.2. |
| `msg` | string | The event's `message` field. |
| `fields` | object | Structured key/value pairs from the emission callsite, post-redaction. |

### 3.2 Span and correlation-ID conventions

- **Boot ID** (`boot` field, every event): UUID v7 minted in `tuxlink::logging::init()` at app start; embedded in every emitted event.
- **Span name** (`span.name`): tracing's span name string (e.g., `dial_attempt`, `b2f_exchange`).
- **Span ID** (`span.id`): tracing's native span ID, hex-formatted.
- **Attempt ID** (`span.attempt_id`): tuxlink convention. Any span representing an operator-meaningful unit of work (a dial attempt, an inbound exchange, a CMS handshake) stamps an opaque short identifier of shape `att-{6-char-base32}`. The Logging window's "last export" line and the auto-generated GitHub issue template both surface the most recent attempt ID for cross-reference.

### 3.3 Export archive layout

```
tuxlink-logs-{UTC-ts}-{attempt-id}.tar.zst       ← outer tarball, zstd level 22, long mode (--long=27), no dictionary
└── (after `zstd -d` + `tar xf`):
    ├── summary.txt              ~200–500 B    paste-friendly headline, plaintext
    ├── events.jsonl.zst         variable      inner zstd, level 22, WITH dictionary
    ├── dict.zdict               ~16 KB        the dictionary used for events.jsonl.zst (also recoverable from the bundled tuxlink binary)
    └── manifest.json            ~700 B        build / OS / policy / counts
```

**Two-layer compression rationale:** the outer `.tar.zst` uses dictionary-free zstd so the agent decompresses with stock `zstd -d` and no extra arguments. The inner `events.jsonl.zst` uses dictionary compression for the win, and ships its dictionary alongside so `zstd -d -D dict.zdict events.jsonl.zst` works on any system with `zstd` installed — no tuxlink-specific tools required.

**Archive filename:**

```
tuxlink-logs-2026-06-04T12-34-56Z-att-xyz1.tar.zst
              \_____UTC ts_____/  \_attempt_/
```

Colons in the UTC timestamp are replaced with dashes so the filename is filesystem-safe on all platforms.

### 3.4 `summary.txt` shape

```
tuxlink-logs export
correlation_id: att-xyz1
exported_at: 2026-06-04T12:34:56Z
window: 2026-05-21T18:21:00Z .. 2026-06-04T12:34:56Z (13d 18h)
events: 3,847 (info: 3,512, warn: 312, error: 23)

build: tuxlink 0.0.1 (git 5fd6cc2, release, linux x86_64)
os: Linux 6.18.29+rpt-rpi-2712 (debian-12)
runtime: tokio 1.41, tauri 2.x

last 3 errors:
  12:34:01.124  winlink::session   dial failed: timeout after 110s; gateway=K6XXX-10
  12:31:18.882  winlink::modem::vara  VARA process exited unexpectedly (signal 9)
  12:14:55.221  winlink::secure   auth challenge timeout

last 5 events:
  12:34:56.789  winlink::session   dial start; transport=vara; gateway=K6XXX-10
  12:34:55.012  ui::menu           Connect clicked
  12:34:32.103  winlink::modem::vara  VARA listener disarmed
  12:33:14.890  position::gpsd     position update; grid=CM87xx
  12:32:01.001  winlink::session   dial succeeded; transport=telnet; gateway=cms-z.winlink.org
```

Plaintext, `grep`-able, no JSON, no escape sequences. The summary.txt is designed for paste into Discord / Winlink message body / SMS multi-part as a bootstrap headline even when the full archive can't transit the channel.

### 3.5 `manifest.json` shape

```json
{
  "v": 1,
  "exported_at": "2026-06-04T12:34:56.789Z",
  "correlation_id": "att-xyz1",
  "window": { "start": "2026-05-21T18:21:00Z", "end": "2026-06-04T12:34:56Z" },
  "build": {
    "version": "0.0.1",
    "git_sha": "5fd6cc2",
    "profile": "release",
    "rust_version": "1.83.0",
    "tauri_version": "2.1.0"
  },
  "platform": {
    "os": "linux",
    "kernel": "6.18.29+rpt-rpi-2712",
    "distro": "debian-12",
    "arch": "x86_64"
  },
  "runtime": {
    "boot_id": "01927a8b-9c12-7000-a4d3-2f8e1b9c0001",
    "boot_at": "2026-06-04T08:00:00Z"
  },
  "logging": {
    "schema_version": 1,
    "redaction_policy_version": 1,
    "detailed_mode": "off",
    "retention_days": 14,
    "retention_mb_cap": 500
  },
  "compression": {
    "outer_algorithm": "zstd",
    "outer_level": 22,
    "outer_long_mode": 27,
    "inner_algorithm": "zstd",
    "inner_level": 22,
    "inner_dict_version": 1
  },
  "counts": { "events": 3847, "info": 3512, "warn": 312, "error": 23 }
}
```

`redaction_policy_version` and `schema_version` let the agent reading the archive know exactly what rules ran. If a future tuxlink release ships a less-aggressive policy, the manifest is the authoritative answer to "which redaction rules were active when this archive was produced."

---

## 4. Verbosity and emission

### 4.1 Per-target verbosity matrix (source of truth)

| Subsystem cluster | Standard mode | Detailed mode |
|---|---|---|
| `winlink::session`, `winlink::secure`, `winlink::handshake`, `winlink::telnet*`, `winlink::transfer` | debug | trace |
| `winlink::modem::ardop`, `winlink::modem::vara`, `winlink::modem::process` | debug | trace |
| `winlink::ax25::frame`, `winlink::ax25::link`, `winlink::ax25::datalink`, `winlink::ax25::kiss`, `winlink::ax25::rfcomm` | debug | trace |
| `winlink::listener::decide`, `winlink::listener::peer`, `winlink::listener::packet_gate`, `winlink::listener::station_password`, `winlink::listener::transport` | debug | trace |
| `winlink::message`, `winlink::proposal`, `winlink::compose`, `native_mailbox` | info | debug |
| `forms::*`, `search::*`, `catalog::*`, `grib::*`, `position::*` | info | debug |
| `wizard`, `bootstrap`, `config`, `tray`, UI command handlers, `ui_commands` | info | debug |
| `logging::env_probes::*` | info | debug |

`error` and `warn` always emit regardless of the Detailed-mode toggle.

### 4.2 What trace adds over debug (per cluster)

- **Transport / modem / AX.25 clusters at trace**: byte-level wire data. TCP send/recv to VARA daemon (with byte counts and short hex preview), KISS-encoded bytes, raw VARA stderr lines, AX.25 frame hex dumps. Useful for "did bytes flow at all" diagnosis when debug-level shows "sent CONNECT, got nothing back."
- **UI / mailbox / forms clusters at debug**: per-render-effect and per-event-handler events. Trace-level for these clusters is intentionally not used in v0 (very noisy, low diagnostic value).

### 4.3 Detailed-mode UI semantics

| State | Behavior |
|---|---|
| **Off** (default) | Per-target matrix's Standard column applies. |
| **On** | Per-target matrix's Detailed column applies. State persists across restarts. |
| **Bounded for N hours** | Detailed mode activates immediately. After N hours (operator-typed, 1–720 bound), state transitions to Off automatically. A `logging.detailed_mode.expired` event emits at revert time. |

State persists in `~/.config/tuxlink/logging.toml` (or `$XDG_CONFIG_HOME/tuxlink/logging.toml`). Schema:

```toml
detailed_mode = "off"           # "off" | "on" | "bounded"
detailed_bounded_expires_at = "2026-06-04T14:48:00Z"  # only when mode = "bounded"
retention_days = 14
retention_mb_cap = 500
```

### 4.4 Emission rollout — big-bang scope

Per operator's "big bang or it doesn't ship" framing, the single PR adds emission calls to every cluster in §4.1. Per-cluster emission discipline:

- **`info` events** mark state-machine milestones an operator would describe in a bug report ("dial start", "dial failed", "session opened", "message sent", "wizard step completed").
- **`debug` events** capture protocol messages and intermediate state ("B2F handshake complete", "VARA command sent: CONNECT", "AX.25 SABM received N(S)=0").
- **`trace` events** (transport / modem / AX.25 clusters only) capture byte-level wire data.
- **`warn` events** mark recoverable surprises ("CMS connection refused; retrying", "AX.25 timer T1 elapsed; retransmitting").
- **`error` events** mark non-recoverable failures the operator should care about ("dial failed: all transports exhausted", "VARA process died").

Each emission site uses the structured-field form (`tracing::info!(transport = %t, gateway = %g, "dial start")`) not the string-interpolation form (`tracing::info!("dial start transport={} gateway={}", t, g)`). Structured fields are essential for the redaction layer and for `jq`-based querying at the agent's end.

### 4.5 Span discipline

The following operations get tracing spans:
- `winlink::session::dial_attempt` — wraps a single dial attempt (one connect through one disconnect)
- `winlink::session::b2f_exchange` — wraps B2F message exchange
- `winlink::secure::auth_handshake` — wraps secure-login challenge/response
- `winlink::listener::inbound_session` — wraps an inbound session from accept through disconnect
- `winlink::transfer::message_send`, `winlink::transfer::message_receive` — per-message
- `forms::http_server::request` — per HTTP request to the form server

Each span carries `attempt_id` as a field. Spans nest naturally — a `b2f_exchange` span inside a `dial_attempt` span gets both span IDs in the event records.

---

## 5. Redaction

### 5.1 Policy version 1: blocklist + marked types

Per operator direction this session: blocklist for first slice (alpha period), allowlist promotion in beta or later. `manifest.json` records `redaction_policy_version: 1` so archives are self-identifying.

### 5.2 Layer A: field-name blocklist

A regex-compiled-once blocklist runs against every field NAME on every emitted event. Match → value replaced with `<redacted>` (the key is preserved so the agent reading the log knows a credential field was present).

```rust
// src-tauri/src/logging/redact.rs

static FIELD_BLOCKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?ix)
        ^(
            password
            | passwd
            | pwd
            | token
            | secret
            | api[_-]?key
            | private[_-]?key
            | station[_-]?password
            | secure[_-]?login[_-]?response
            | secure[_-]?login[_-]?challenge
            | challenge[_-]?response
            | auth[_-]?header
            | bearer
            | keyring[_-]?value
            | credential
            | session[_-]?cookie
        )$
    ").expect("redaction blocklist regex must compile")
});

pub fn should_redact_field(name: &str) -> bool {
    FIELD_BLOCKLIST.is_match(name)
}
```

The regex is **anchored** (`^...$`) so plausibly-benign field names like `password_hint_index` do not match. Adding a new sensitive key is a one-line change to the regex.

### 5.3 Layer B: custom `Debug` on credential types

Belt-and-suspenders: even if someone writes `tracing::debug!(?creds, "auth state")` (passing the whole struct), the output is `<redacted CredentialsType>` not the inner fields.

Structs receiving the custom `Debug` impl (audit list, verified against current `src-tauri/src/` source):

- `winlink::credentials::WinlinkCredentials`
- `winlink::credentials::StationPassword` (if present in `winlink::credentials`)
- `winlink::secure::SecureLoginResponse`
- `winlink::secure::SecureLoginChallenge`
- `winlink::listener::station_password::StationPassword`
- Any future struct that stores or transmits a credential — adding a new credential-bearing type without the `Debug` impl is a defect.

Each implementation is one-line and mechanical:

```rust
impl std::fmt::Debug for StationPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted StationPassword>")
    }
}
```

### 5.4 Scope — what gets redacted, what does not

Per operator direction this session: redaction scope is **passwords and credentials only**. The following are NOT redacted (emit verbatim):

- Callsigns (own and peer) — public via FCC ULS, diagnostically essential.
- Position / lat-lon — emitted at whatever precision the position subsystem produces (already operator-controlled via the GPS & Privacy panel).
- Local file paths — diagnostically useful, not credential-bearing.
- Message body / subject / headers — not redacted in v0; if operator scope expands in beta, revisit.
- Hostnames — emitted as-is.
- IP addresses — emitted as-is (CMS server addresses are public).

### 5.5 Promotion path (alpha → beta)

The design supports future allowlist promotion without architectural change:
- `redaction_policy_version: 1` in manifest = current blocklist policy.
- `redaction_policy_version: 2` (future) = allowlist policy.
- Same code path; the `RedactionPolicy` enum variant swap changes the Visitor's logic.

### 5.6 Required tests

- **Unit**: every name in the blocklist matches the regex; control case `password_hint_index` does NOT match.
- **Unit**: every credential-bearing struct's `Debug` impl returns the `<redacted ...>` form and does not include inner-field values.
- **Integration**: emit a worst-case `tracing::debug!(password = %real_pw, ...)` event, run the full pipeline, assert the resulting events.jsonl contains `"password":"<redacted>"` and does NOT contain the real password string.
- **Integration**: emit an event embedding a credential struct (`tracing::debug!(creds = ?creds, ...)`), assert events.jsonl contains the `<redacted StationPassword>` form.

---

## 6. Storage

### 6.1 Location

Logs live at `$XDG_STATE_HOME/tuxlink/logs/` (typically `~/.local/state/tuxlink/logs/`). State_home is correct per the XDG Base Directory Specification — logs are state, not cache; they survive `rm -rf ~/.cache` and cache-clearing tools.

Fallback for systems missing `$XDG_STATE_HOME`: `$HOME/.local/state/tuxlink/logs/`. The `dirs` crate's `state_dir()` lookup handles cross-platform resolution.

### 6.2 Rolling strategy

`tracing-appender::rolling::Builder` with `Rotation::HOURLY` and UTC-midnight-aligned hour boundaries:

```rust
let file_appender = tracing_appender::rolling::Builder::new()
    .rotation(Rotation::HOURLY)
    .filename_prefix("tuxlink")
    .filename_suffix("jsonl")
    .build(state_home.join("tuxlink/logs/"))
    .expect("log directory must be creatable");

let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
```

File naming follows `tracing-appender`'s default hourly format: `tuxlink.YYYY-MM-DD-HH.jsonl` (UTC-timestamped).

Properties:
- **Hourly rotation** at UTC hour boundaries.
- **Non-blocking writes** — events queue to a background thread; emission callsites never block on disk I/O.
- **Atomic per-event writes** — each line is a single write call; partial writes do not happen.
- **Crash-resistant** — unflushed events in the buffer (last ~1s of activity) are lost on hard crash; no file corruption.

### 6.3 Retention enforcement

`tuxlink::logging::retention::sweep` runs:
- At startup, before the appender opens.
- After each hour rotation.
- Immediately when the operator changes retention values via the Logging window.

Sweep logic:
1. List `tuxlink-*.jsonl` files in the log directory, sorted by filename (which sorts by UTC hour because of the ISO timestamp prefix).
2. Compute current total size.
3. Determine cutoff via two rules, take the more aggressive:
   - **Days rule**: files older than `retention_days` days → delete.
   - **Size rule**: if total size exceeds `retention_mb_cap`, delete oldest files until under cap.
4. Delete files outside the cutoff. Emit `retention sweep: deleted N files (X MB), retained Yd Zh / W MB` at `info`. The sweep itself appears in the log.

Retention bounds: `retention_days` 1–365; `retention_mb_cap` 50 MB – 10 GB.

### 6.4 Free-disk guard

Regardless of the configured retention cap, when `$XDG_STATE_HOME`'s filesystem reports less than 100 MB free, the disk layer:
1. Emits `warn`-level `disk-space-low: stopping log writes; free=X MB`.
2. Stops queueing new events to the appender (events still flow to the UI subscriber).
3. Re-checks every 5 minutes; resumes when free space recovers above 200 MB.

This protects against the pathological case of a runaway debug session on a Pi with a tight SD card filling everything else on the disk.

### 6.5 Concurrency model

- **Writer**: tracing-appender's non-blocking appender owns the file handle. Single writer thread; lock-free MPMC queue feeds it from emission sites.
- **Reader (export pipeline)**: opens each `tuxlink-*.jsonl` file read-only. The current-hour file may be actively being written; `read()` stops at EOF (the last fully-written event). Events arriving during export are not in the export but are durably on disk for the next export.
- **Retention sweeper**: serialized with the writer via mutex so it cannot delete a file the writer just opened. Sweep is fast (<50ms typical).

---

## 7. Compression

### 7.1 v0 strategy

- **Outer tarball** (`tuxlink-logs-*.tar.zst`): zstd level 22 with long-range mode (`--long=27`). No dictionary. Standard `zstd -d` decompresses without flags.
- **Inner events stream** (`events.jsonl.zst` inside the tarball): zstd level 22 with the bundled dictionary. Dictionary embedded in the archive as `dict.zdict` so `zstd -d -D dict.zdict events.jsonl.zst` works at the agent's end with no tuxlink installed.

### 7.2 v0 dictionary

`src-tauri/assets/logging/tuxlink-events-v1.zdict` (~16 KB), trained from a synthetic corpus generated by the `xtask gen-corpus` tool. Dictionary is bundled into the tuxlink binary via `include_bytes!`. The same dictionary is shipped inside every export archive (as `dict.zdict`) so archives are self-decompressing.

### 7.3 Synthetic corpus (v0 training input)

`xtask/src/bin/gen-corpus.rs` produces approximately 1.5–2 MB of representative JSONL events at `dev/log-corpus-synthetic/` (gitignored). Coverage:
- Dial attempt event sequences across all three transports (telnet, ARDOP, VARA)
- B2F handshake event sequences
- Modem command/response exchanges (ARDOP and VARA)
- AX.25 frame events (SABM, UA, I-frame, RR, DISC)
- Listener inbound-session events
- All six environment probe outputs with realistic value variation
- Wizard / bootstrap / config events
- Error variations across each subsystem (timeouts, refused, malformed, unauthorized)

Variation includes multiple callsigns, multiple correlation IDs, multiple gateway names, multiple frequency values, multiple error message strings, multiple timestamp distributions.

### 7.4 v1 dictionary upgrade path

When alpha collects ~5–10 MB of real-corpus data over a few weeks:
1. Operator runs `cargo xtask train-log-dict --input dev/log-corpus-real --output src-tauri/assets/logging/tuxlink-events-v2.zdict --size-kb 32`.
2. Bump the `include_bytes!` filename in `src-tauri/src/logging/dict.rs` to `v2`.
3. Bump `dict_version` constant.
4. Ship.

The infrastructure code does not change. v1 is a single asset add + three-line code change.

### 7.5 Dictionary-mismatch fallback

If `dict.zdict` is corrupt or missing when the disk layer initializes, zstd falls back to dictionary-free compression. A `warn`-level event records the fallback. Archives produced in fallback mode set `inner_dict_version: null` in `manifest.json`.

### 7.6 Decompression at the agent's end

```bash
zstd -d tuxlink-logs-XXX.tar.zst -o tuxlink-logs-XXX.tar
tar xf tuxlink-logs-XXX.tar
# yields: summary.txt, events.jsonl.zst, dict.zdict, manifest.json
zstd -d -D dict.zdict events.jsonl.zst -o events.jsonl
jq '.target' events.jsonl | sort -u   # see which clusters emitted
```

Requires only stock `tar` and `zstd`. No tuxlink-specific tools.

---

## 8. UI

### 8.1 Window vs panel

The Logging surface lives in a **separate Tauri window** (`logging` label), mirroring the precedent set by `help_window.rs` and `compose_window.rs`. The window pattern is permitted here per operator direction this session: Logging is infrequent admin (not part of the routine compose/send/receive workflow that motivated `inline-ui-no-window-clutter`).

Window properties (mirror `help_window.rs`):
- Single-instance — re-invoking `logging_window_open` focuses the existing window.
- Main-window-only invoker guard.
- Idempotent on re-invoke.
- `WindowLabelAlreadyExists` race-guard.
- Custom in-app titlebar (`decorations: false`), matching the existing Tuxlink chrome.
- Geometry persisted via `tauri-plugin-window-state` (already a dependency).
- Inner size: 820 × 720; min size: 600 × 480.

### 8.2 Window layout — one scrollable view, three sections

The window renders ONE scrollable view with three named sections in vertical order. No tabs (rejected as a Geographica-shaped pattern; Tuxlink's main UI uses panels + sections, not tabs).

Section order:

1. **Export** — status (disk usage, retained window, event rate, last export info with attempt-ID + Copy link), `Export logs…` primary button, `Open log directory` secondary action, `Clear history…` destructive action with confirmation.
2. **Settings** — Detailed-mode three-radio (Off / On / Bounded for N hours); Retention number inputs (days input 1–365; disk cap number input + MB/GB unit selector, bounded 50 MB – 10 GB).
3. **Environment probes** — last snapshot inline (status dot + one-line summary per probe), `Re-run probes` action.

Visual aesthetic: flat sections with horizontal-rule separators between sections, plain section headers, no rounded card containers. Follows Tuxlink's existing radio-panel / dashboard-ribbon conventions. Implementation references existing Tuxlink components (DashboardRibbon's color tokens, the panel-header conventions used in Settings → GPS & Privacy) so the result lands visually consistent with the rest of the app.

### 8.3 Help menu wiring

`src/shell/chrome/menuModel.ts` Help submenu becomes:

```typescript
{ label: 'Help', items: [
  { id: 'menu:help:about',         label: 'About Tuxlink' },
  { id: 'menu:help:docs',          label: 'Documentation' },     // existing → help_window
  { id: 'menu:help:logging',       label: 'Logging…' },          // NEW → logging_window
  { id: 'menu:help:report_issue',  label: 'Report Issue' },      // existing label; new behavior
] }
```

`src/shell/chrome/dispatchMenuAction.ts`:
- `menu:help:logging` → invoke `logging_window_open` Tauri command.
- `menu:help:report_issue` → invoke `report_issue_flow` Tauri command (defined below in §8.5).

### 8.4 Tauri commands (backend)

New commands registered in `src-tauri/src/lib.rs`:

| Command | Caller | Returns | Description |
|---|---|---|---|
| `logging_window_open` | main | `Result<(), String>` | Opens or focuses the Logging window. Same guard pattern as `help_window_open`. |
| `logging_status` | logging | `LoggingStatus` (disk usage, retained window, event rate, last export, current Detailed-mode state, retention values) | Read-only snapshot for the Export section. |
| `logging_set_detailed_mode` | logging | `Result<(), String>` | Off / On / Bounded(hours). Persists to settings; re-applies the filter layer. |
| `logging_set_retention` | logging | `Result<(), String>` | Sets retention_days + retention_mb_cap. Triggers immediate sweep. |
| `logging_clear_history` | logging | `Result<(), String>` | Wipes the ring buffer + deletes all rolled files. Operator confirmation required UI-side. |
| `logging_open_directory` | logging | `Result<(), String>` | Opens the log directory in the OS file manager via `tauri-plugin-shell::open`. |
| `logging_export` | logging or main | `Result<ExportResult, String>` (with file path) | Builds the archive, writes via Save As dialog (`tauri-plugin-dialog`). |
| `logging_env_probes_snapshot` | logging | `EnvProbesSnapshot` | Returns the last snapshot; emits a fresh probe run if older than 60s. |
| `logging_env_probes_rerun` | logging | `EnvProbesSnapshot` | Forces fresh probe run. |
| `report_issue_flow` | main | `Result<ReportIssueResult, String>` | Runs `logging_export` then opens browser to pre-filled GitHub Issues URL. |

The `caller` column documents which window may invoke each command (defense in depth against a misbehaving frontend).

### 8.5 Report Issue flow

`Help → Report Issue` triggers `report_issue_flow`:

1. Invokes `logging_export` to produce the archive (Save As dialog opens; operator chooses location).
2. On successful export, opens browser via `tauri-plugin-shell::open` to:

   ```
   https://github.com/cameronzucker/tuxlink/issues/new?labels=alpha-report&body=<URL-encoded body>
   ```

3. The body template substitutes runtime values for `{version}`, `{git_sha}`, `{os}`, `{kernel}`, `{correlation_id}`, `{exported_at}`, `{archive_path}`, `{archive_size}`:

   ```markdown
   <!-- tuxlink auto-generated bug report template -->

   **Build:** tuxlink {version} (git {git_sha}, release)
   **Platform:** {os} · {kernel}
   **Correlation ID:** {correlation_id}
   **Exported at:** {exported_at}

   **📎 Log archive saved at:** `{archive_path}` ({archive_size})

   👉 **Please drag the file above into this comment box now** so it attaches to the issue. (GitHub will upload it.)

   ---

   ## What happened
   (Describe what you were trying to do, what happened instead, and what you expected.)

   ## Steps to reproduce
   1.
   2.
   3.

   ## Anything else
   (Screenshots, related context, anything you noticed.)
   ```

4. A brief inline modal in the main window shows during the flow: "Log export saved to {path} · Opening GitHub Issues in your browser…" with a Copy-path button + manual Open-browser fallback.

### 8.6 GitHub issue template file

A new `.github/ISSUE_TEMPLATE/bug.md` file in the repo carries the same template structure so users who reach GitHub directly (not via the Tuxlink button) see the same shape.

### 8.7 Single source of truth for export

Both `Help → Logging → Export logs…` and `Help → Report Issue` invoke the SAME `tuxlink::logging::export::build_archive()` function. There is no second code path. A bug in the export pipeline surfaces identically through both menu items — fewer surfaces to QA.

---

## 9. Environment probes — hard alpha requirement

Per operator direction this session: probe coverage is a release-gate criterion, not nice-to-have. The reasoning: without proactive environmental capture, an alpha tester with a broken keyring (or audio device, or serial port) gets logs containing the SYMPTOM but not the CAUSE. The diagnosing agent's next step becomes "send me the output of `systemctl --user status gnome-keyring-daemon` and `echo $DBUS_SESSION_BUS_ADDRESS`" — a multi-round back-and-forth with someone who lacks the Linux skill to answer.

### 9.1 Probe trigger pattern

Each probe runs:
- **At app startup** (once): baseline snapshot, emitted as `info`-level events.
- **On first `error`-level event from its subsystem**: probe re-runs and emits a sibling event correlated with the error via `attempt_id`.

The probe-on-error pattern is triggered by a Layer in the subscriber composition that inspects the event's `target` and dispatches to the matching probe.

### 9.2 The six v0 probes

| Probe | `target` | Captures |
|---|---|---|
| **keyring** | `tuxlink::logging::env_probes::keyring` | compile_features, resolved backend, DBus session-bus address presence + reachability, XDG_RUNTIME_DIR existence + perms, `gnome-keyring-daemon` / `kwallet` / `keepassxc` systemd-active state, `~/.local/share/keyrings/` existence + owner-uid match, default collection lock state, tuxlink entries count |
| **audio** | `tuxlink::logging::env_probes::audio` | ALSA/PipeWire reachability, active device list, configured device name match, sample-rate support, DigiRig detection |
| **serial** | `tuxlink::logging::env_probes::serial` | `/dev/serial/by-id` listing, configured port existence, permissions, user `dialout` group membership |
| **modem_process** | `tuxlink::logging::env_probes::modem_process` | VARA process state (running, last exit code, signal, stderr tail if available), ARDOP process state same |
| **network** | `tuxlink::logging::env_probes::network` | DNS resolution for `cms-z.winlink.org`, route to CMS reachable, last successful CMS contact timestamp |
| **display** | `tuxlink::logging::env_probes::display` | `WAYLAND_DISPLAY` / `DISPLAY` presence, WebKitGTK version, GPU vendor string |

### 9.3 Probe-specific redaction

Probes use an **explicit allowlist of environment-variable NAMES** they read:
- `XDG_*` family
- `DBUS_SESSION_BUS_ADDRESS`
- `DESKTOP_SESSION`
- `HOME`, `USER`
- `WAYLAND_DISPLAY`, `DISPLAY`
- `LANG`, `LC_*`

Probes never dump `std::env::vars()`. This prevents a secret-in-env-var (rare but possible: `WINLINK_PASSWORD=...` exported for a sibling tool) from leaking into probe output. The allowlist lives in `src-tauri/src/logging/env_probes/mod.rs::ENV_ALLOWLIST` as a const slice.

### 9.4 Probe ordering and cost

Probes run sequentially at startup (not parallel) so the startup-snapshot event appears as one ordered block in the log. Total startup cost target: <250ms. Individual probes that exceed 100ms log a `warn` and continue.

### 9.5 Probe results in the Logging window

The Logging window's Environment-probes section displays the most recent snapshot summary inline (status dot + one-line description per probe). `Re-run probes` triggers `logging_env_probes_rerun`. This gives operators self-service environmental diagnostics without exporting a log.

---

## 10. Acceptance criteria

The PR is mergeable when ALL of the following hold:

### 10.1 Functional

1. `tracing` macros (`info!`, `debug!`, `warn!`, `error!`) emit from every cluster in the §4.1 matrix.
2. The Logging window opens from `Help → Logging…` with all three sections, all controls operable.
3. `Help → Report Issue` triggers auto-export, opens browser to the pre-filled GitHub Issues URL, displays the modal with file path + Copy-path button.
4. Export produces `tuxlink-logs-{UTC-ts}-{attempt-id}.tar.zst` containing `summary.txt`, `events.jsonl.zst`, `dict.zdict`, `manifest.json` per §3.3–§3.5.
5. Retention sweep deletes oldest files when either cap is hit; sweep events appear in the log.
6. Detailed mode auto-reverts after the operator's chosen window; revert event (`logging.detailed_mode.expired`) appears in the log.
7. Six environment probes emit at startup AND on first error from their respective subsystems; probe outputs contain only allowlisted env-var values.

### 10.2 Redaction-safety (correctness-critical, must have tests)

8. **Unit**: every name in §5.2 blocklist matches the regex; `password_hint_index` (control case) does NOT match.
9. **Unit**: every credential-bearing struct from §5.3 has a `Debug` impl returning the `<redacted ...>` form and not including inner-field values.
10. **Integration**: a worst-case `tracing::debug!(password = %real_pw, ...)` event, run through the full pipeline, produces events.jsonl containing `"password":"<redacted>"` and NOT containing the real password string.
11. **Integration**: an event embedding a credential struct (`tracing::debug!(creds = ?creds, ...)`) produces events.jsonl containing `"creds":"<redacted StationPassword>"` (or the matching type name).

### 10.3 Decompression portability (agent-end ergonomics)

12. Archive decompresses with stock `tar` + `zstd` only: `zstd -d archive.tar.zst && tar xf archive.tar && zstd -d -D dict.zdict events.jsonl.zst` produces valid JSONL.
13. `summary.txt` is plain text, `grep`-readable, no escape sequences or binary content.

### 10.4 Smoke artifacts

14. `scripts/tuxlink-logging-smoke.sh` exists, exits 0 on success, exercises: app starts → env probes emit → synthetic event sequence → Export → unpack archive → verify summary.txt + events.jsonl content. Agent-runnable, zero RADIO-1 risk. Analogous shape to `scripts/tuxmodem-loopback-smoke.sh`.

### 10.5 Build pipeline

15. `cargo xtask gen-corpus` produces ~1.5–2 MB of synthetic events at `dev/log-corpus-synthetic/`.
16. `cargo xtask train-log-dict` produces `src-tauri/assets/logging/tuxlink-events-v1.zdict` (~16 KB).
17. Both xtask binaries documented in new `xtask/README.md`.

### 10.6 Adversarial review

18. At least one Codex adversarial round per `superpowers:build-robust-features` discipline, given correctness-critical surface area (redaction logic, dictionary training corpus completeness, env-probe allowlist completeness). Per memory `no-carveout-on-cross-provider-adrev`.

---

## 11. Operator smoke plan (<10 minutes)

Runnable once the PR merges and the converged build is up:

```bash
# 1. Confirm app starts and env-probes fire at startup.
pnpm tauri dev
# In another terminal:
tail -f ~/.local/state/tuxlink/logs/tuxlink-*.jsonl | grep env_probe

# 2. Open Help → Logging. Verify window opens with Export / Settings / Environment-probes sections.

# 3. Click "Export logs…" → save to /tmp/test.tar.zst.
#    Verify file exists; verify size is reasonable (KB-class for a fresh install).

# 4. Decompress the archive without tuxlink:
cd /tmp && mkdir x && cd x && zstd -d ../test.tar.zst && tar xf test.tar
cat summary.txt                                # should be human-readable
zstd -d -D dict.zdict events.jsonl.zst -o events.jsonl
head events.jsonl                              # valid JSONL, env_probe events visible
jq '.target' events.jsonl | sort -u            # see emitting clusters

# 5. In the Logging window, enable Detailed mode Bounded for 1 hour. Verify time-remaining displays.

# 6. Wait a few minutes; click Help → Report Issue.
#    Verify modal appears with file path; browser opens with pre-filled GitHub issue template;
#    drag-drop instructions visible.

# 7. Optional: shrink retention to 1 day / 50 MB in Logging window.
#    Verify a retention sweep event appears in the log within a few seconds.

# 8. Run the agent-runnable smoke script:
bash scripts/tuxlink-logging-smoke.sh
# expect: exit 0, "PASS" output
```

---

## 12. Out-of-scope deferrals

Explicitly NOT in the first-slice PR:

- **Real-corpus dictionary retraining** — v1 (single asset swap).
- **Per-subsystem verbosity sliders** — post-alpha; depends on operator demand surfacing.
- **In-app log viewer** — separate UI work; existing radio-panel session-log strip handles live tailing.
- **Allowlist-based redaction promotion** — beta or later; depends on surface area becoming too big for per-callsite review.
- **`gh` CLI detection / GitHub PAT integration** — post-alpha if URL-pre-fill friction surfaces.
- **Per-attempt level elevation** — earlier brainstorm proposal; explicitly rejected in favor of the simpler Off/On/Bounded operator-facing model.
- **Tiered exports (TINY / COMPACT / FULL picker)** — earlier brainstorm proposal; rejected as a foot-gun per operator direction.

These each become bd-issue follow-ups during `superpowers:writing-plans` decomposition.

---

## 13. Risk acknowledgment

- **Big-bang PR shape is heavy.** Mitigation: comprehensive acceptance criteria above, runnable smoke script, redaction unit + integration tests, env probes covered explicitly as alpha-gate requirements. Per operator framing this session: "ships as a working feature or it doesn't."
- **Synthetic-corpus-trained dictionary may compress real logs worse than ideal.** Mitigation: zstd dictionary mode falls back to dictionary-free compression on unmatched patterns — floor is "dict is wasted weight," not "dict actively hurts." v1 retraining from real corpus is a single asset swap.
- **Codex adversarial round is mandatory.** Redaction logic + dictionary training corpus + env-probe allowlists each warrant independent scrutiny. Per memory `[[no-carveout-on-cross-provider-adrev]]` this is correctness-critical territory, not plumbing.
- **Existing `SessionLogState` ring buffer reuse** keeps the radio-panel session-log strip working without React-side change, but the new ui_layer must construct `LogLine` identically to the existing winlink_backend bridge or live-events will diverge in shape. Mitigation: a parity test asserts that the new layer's output for a given tracing event equals the existing bridge's output for the equivalent legacy emission.

---

## 14. Memory references

This design observes:
- `alpha-is-vettedness-not-built-ness` — first-slice ships fully built including env probes
- `no-tuxlink-added-safeguards` — applies to TX-path behavior; logging has no TX surface
- `inline-ui-no-window-clutter` — exception justified per §8.1 (infrequent admin, mirrors `help_window.rs`)
- `no-disk-creds-default` — drives the redaction layer's password-only scope
- `no-incomplete-or-internal-refs-in-shipped-features` — env probes shipped together with the export feature (rather than as a placeholder for a follow-up PR)
- `discipline-triage-rule` — Codex round IS warranted here (correctness-critical), unlike pure plumbing
- `no-carveout-on-cross-provider-adrev` — §10.6 mandates the Codex round
- `no-stretched-full-width-ui` — Logging window constrained, no full-width stretch
- `explicit-referents-in-specs` — this spec names the feature + state at every reference

---

## 15. Implementation rollout (decomposition target for writing-plans)

The big-bang PR breaks into the following logical groupings for the implementation plan (one-PR scope, but the plan ordering matters for review structure):

1. **Infra foundation** — `logging/` module skeleton, Subscriber composition, filter layer, redaction layer, env-probes trait + dispatch.
2. **Disk layer + retention** — `tracing-appender` integration, rolling files, sweep, free-disk guard.
3. **Export + compression** — `xtask` skeleton, gen-corpus, train-log-dict, asset bundling, archive builder.
4. **Six env probes** — keyring, audio, serial, modem_process, network, display.
5. **Logging window (backend)** — `logging_window.rs`, Tauri commands, settings persistence.
6. **Logging window (frontend)** — `/logging` route, LoggingView with Export / Settings / Env-probes sections.
7. **Report Issue flow** — `report_issue_flow` command, GitHub URL pre-fill, modal in main window, `.github/ISSUE_TEMPLATE/bug.md`.
8. **Emission rollout** — `tracing::*!` calls across every cluster in §4.1; spans on §4.5 operations.
9. **Tests** — redaction unit + integration; export round-trip; smoke script.
10. **Codex adversarial round** — per §10.6.

`superpowers:writing-plans` will translate these into the actual implementation plan + bd-issue decomposition.

---

**End of spec.**

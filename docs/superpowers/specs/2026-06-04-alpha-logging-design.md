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
- Dedicated in-app diagnostic log viewer with filter/query — separate UI work; the radio-panel session-log strip is reserved for explicit connection/session narration, not general diagnostic live-tail
- `gh` CLI detection / GitHub PAT integration — post-alpha if URL-pre-fill friction surfaces

---

## 2. Architecture

### 2.1 Stream model

**One diagnostic `tracing` stream, disk/export as the default rendering.** Emission sites use `tracing` macros (`info!`, `debug!`, `warn!`, `error!`); the subscriber composition routes diagnostic events to disk as JSONL. The radio-panel `SessionLogState` ring buffer is not a diagnostic catch-all: events reach it only through explicit session-log APIs, or through a tracing event that deliberately opts in with `session_log=true`.

2026-06-07 smoke correction (`tuxlink-pzak`): routing every `tracing` event into `SessionLogState` made the connection log show startup diagnostics like `gpsd connected`, `bootstrap action decided`, and env-probe snapshots. That is the wrong operator surface. The connection log remains connection/session narration; diagnostic startup context belongs in the logging archive/window.

### 2.2 Pipeline

```
                ┌──────────────────────────────────────────────────┐
                │  Emission callsites (tracing::info!, …)          │
                │  ~70 src-tauri modules                           │
                │  Wire-text callsites use the WireSanitizer       │
                │  helper FIRST to redact wire-line secrets        │
                │  (;PR:, Password:-response, peer_password) BEFORE│
                │  passing the string to a tracing macro (§5.6)    │
                └────────────────┬─────────────────────────────────┘
                                 │
                  ┌──────────────▼─────────────────────┐
                  │ tuxlink::logging::Subscriber       │
                  │ - Filter Layer (per-target levels  │
                  │   + Detailed-mode toggle)          │
                  │ - Fanout Layer (single point that  │
                  │   formats each event ONCE through  │
                  │   a redacting `Visit`, allocates a │
                  │   monotonic seq ONCE, then         │
                  │   broadcasts the redacted          │
                  │   `LoggedEvent` to consumers)      │
                  └──────────────┬─────────────────────┘
                                 │ broadcast::Sender<LoggedEvent>
                  ┌──────────────┴───────────────────┐
                  ▼                                  ▼
         ┌──────────────────┐               ┌───────────────────┐
         │ UI consumer task │               │ Disk consumer task│
         │ - opt-in only:   │               │ - writes redacted │
         │   session_log=   │               │   JSONL line to   │
         │   true events    │               │   tracing-appender│
         │   may append     │               │   (non-blocking)  │
         └────────┬─────────┘               └──────────┬────────┘
                  ▼                                    ▼
        Existing radio panel              $XDG_STATE_HOME/tuxlink/logs/
        connection/session log            tuxlink.YYYY-MM-DD-HH.jsonl
        strip, not diagnostic             (perms 0600, dir perms 0700)
        general live-tail                         │
                                                  │
                                                  ▼
                                        ┌───────────────────────────┐
                                        │ Export builder            │
                                        │ - flush barrier across the│
                                        │   fanout/appender pipeline│
                                        │ - read closed files; tail │
                                        │   active file safely      │
                                        │ - render summary.txt      │
                                        │ - render manifest         │
                                        │ - inner: zstd+dict        │
                                        │ - outer: tar.zst          │
                                        └────────────┬──────────────┘
                                                     ▼
                                        tuxlink-logs-{ts}-{corr-id}.tar.zst
```

**Architectural property: single-format, single-fanout.** `tracing` events are immutable once emitted; a `Layer` cannot mutate fields that downstream Layers see. The pipeline therefore formats each event **exactly once** through a redacting `tracing::field::Visit` implementation, producing a `LoggedEvent` struct (the post-redaction representation). That single redacted representation is broadcast to UI and disk consumers. There is NO architectural path where the disk consumer can see a different set of field values than the UI consumer. There is NO path where a credential value bypasses redaction by reaching one consumer first.

**Allocation property: seq is assigned once before fanout.** The Fanout Layer allocates the monotonic `seq` (using the existing `SessionLogState::next_seq` counter via a dedicated `allocate_seq()` helper that bumps without appending), stamps it on the `LoggedEvent`, and broadcasts. The UI consumer calls `SessionLogState::append_with_seq(seq, line)` (new API) which appends WITHOUT re-allocating. The disk consumer writes the same seq into the JSONL line. UI strip and archive cross-reference correctly with no double-bump risk.

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

**New standalone `xtask` crate under `xtask/`:**

```
xtask/
├── Cargo.toml
├── README.md                Documents both binaries
└── src/bin/
    ├── gen-corpus.rs        Synthetic event-corpus generator
    └── train-log-dict.rs    zstd::dict::from_files() driver
```

`xtask` is intentionally not a repository-root Cargo workspace member. Invoke it
with `cargo run --manifest-path xtask/Cargo.toml --target-dir xtask/target ...`
so helper builds never create `/target` at the repository root.

**New asset:**

```
src-tauri/assets/logging/
└── tuxlink-events-v1.zdict  ~16 KB, synthetic-corpus-trained
```

**New frontend route:** `/logging` rendering `src/help/LoggingView.tsx`.

**Touched files (full inventory; v1 underestimated this — per Codex §6 Finding 1):**

Backend (src-tauri/src/):
- `lib.rs` — register all logging Tauri commands + `logging_window_open` + start the disk-consumer task with the appender guard stored in Tauri-managed state (the SINGLE init owner; see §2.6 lifecycle).
- `main.rs` — does NOT initialize the subscriber (corrected from v1; subscriber init lives wholly inside `logging::init()` called by `lib.rs::run()`).
- `Cargo.toml` — add tracing + tracing-subscriber + tracing-appender + zstd + tar + once_cell deps.
- Every src-tauri module the §4.1 matrix names — add `tracing` imports + emission calls per §4.4 callsite policy.

Helper bins (src-tauri/src/bin/):
- `native_cms_probe.rs` and `vara_tcp_probe.rs` — explicitly OUT OF SCOPE for v0 logging integration. They will continue to write to stderr; their bug reports are excluded from the export until a follow-up. Documented in §12.

tuxmodem workspace:
- `tuxmodem/` — OUT OF SCOPE for v0. The tuxmodem CLI tools are separate processes outside the Tauri app subscriber. A future PR adds a compatible JSONL emitter for tuxmodem if alpha-tester bug reports surface modem-CLI demand.

Frontend (src/):
- `shell/chrome/menuModel.ts` — add `menu:help:logging`, keep `menu:help:report_issue` id (behavior changes).
- `shell/chrome/dispatchMenuAction.ts` — route the two help actions to the new Tauri commands.
- `routing.ts` — add `/logging` route parser (currently routes only handle `/compose/*` and `/help`).
- `App.tsx` — add lazy branch for `/logging` rendering `LoggingView`.
- `help/LoggingView.tsx` (NEW) — the window body's three sections.
- `help/ReportIssueModal.tsx` (NEW) — main-window transient modal during the auto-export → browser-open transition.
- `help/LoggingView.test.tsx` (NEW) — component tests.
- `routing.test.ts` (existing) — extend with `/logging` parse tests.

Repo root:
- `xtask/` (NEW crate) — contains `gen-corpus.rs`, `train-log-dict.rs`, `README.md`.
- `.github/ISSUE_TEMPLATE/bug.md` (NEW) — mirrors the in-app GitHub URL template.
- `scripts/tuxlink-logging-smoke.sh` (NEW) — agent-runnable smoke per §10.4.

### 2.5 Reuse of `session_log.rs`

The existing `SessionLogState` ring buffer (`src-tauri/src/session_log.rs`) is unchanged in its public read API. The radio-panel session-log strip continues to read from `SessionLogState` via `session_log_snapshot` and the existing broadcast channel — no React-side changes.

The `SessionLogState` impl gains a new `append_with_seq(seq: u64, line: LogLine)` method that appends without bumping the internal `next_seq` counter. The `Fanout Layer` (see §2.2) is the SINGLE allocator of diagnostic `seq`: it bumps via a new `allocate_seq()` helper exactly once per event, stamps the value on the `LoggedEvent` broadcast payload, and the UI consumer task may call `append_with_seq(stamped_seq, line)` only for events that explicitly opt in to the connection/session log with `session_log=true`.

This eliminates the v1-spec race where independent UI and disk Layers could both touch the counter (Codex §8 Finding 1).

### 2.6 Lifecycle and init ownership (Codex §9 Finding 1, §12 Finding 3)

Single init owner: `tuxlink::logging::init(app: &mut tauri::App) -> Result<LoggingHandle, LoggingInitError>`. Called exactly once during Tauri builder `.setup()`. Returns a `LoggingHandle` carrying:
- The `tracing_appender::non_blocking::WorkerGuard` — must live for process lifetime; dropped on app exit.
- An `Arc<LoggingState>` carrying the `Arc<SessionLogState>`, the broadcast `Sender<LoggedEvent>`, the reload-handle for the filter layer (for runtime Detailed-mode flips), the active-file-path tracker (for retention sweep's "never delete active" rule), and the persisted `Settings` snapshot.

The `LoggingHandle` (containing the guard) is stored in `app.manage(handle)`. Tauri-managed state outlives the builder closure; the guard remains held for process lifetime; the broadcast Sender is cloneable for the disk-consumer task.

`main.rs` does NOT call any tracing init function. The previous v1-spec text saying both `lib.rs` and `main.rs` initialize logging was wrong; the corrected lifecycle has exactly one owner (`logging::init()` invoked from `lib.rs::run()`'s setup hook).

For very-early-startup tracing (before Tauri builder is ready), `lib.rs::run()` installs a temporary `fmt::Subscriber` writing to stderr only. Once `logging::init()` returns, the stderr subscriber is unset and the full subscriber takes over. Events from the early window are NOT captured in JSONL (acceptable; the early window covers parse of config + Tauri builder construction, no operational meaning).

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
  "module": "tuxlink::winlink::session",
  "file": "src-tauri/src/winlink/session.rs",
  "line": 412,
  "pid": 12345,
  "thread": { "id": 7, "name": "tokio-runtime-worker" },
  "attempt_id": "att-xyz1",
  "spans": [
    { "name": "dial_attempt", "id": "0x7f3a" },
    { "name": "b2f_exchange", "id": "0x812c" }
  ],
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
| `v` | integer | Schema version. `1` for first release. Bumped on breaking change. Additive field additions within `v:1` are permitted; type changes require `v:2`. |
| `ts` | string (RFC3339) | UTC, microsecond precision. Required. |
| `boot` | string (UUID v7) | Minted at process start in `logging::init()`. Lives on the `Subscriber`. Unique per process launch. Required. |
| `seq` | integer | Monotonic; allocated by the Fanout Layer's `allocate_seq()` (Codex §8 Finding 1 — single allocator, never double-bumped). Required. |
| `level` | string enum | `trace` \| `debug` \| `info` \| `warn` \| `error`. Required. |
| `target` | string | Tracing target string. Required. |
| `module` | string | `module_path!()` from the emission callsite. Optional; same as `target` in most cases but differs for re-exports. |
| `file` | string | `file!()` from the emission callsite, repo-relative. Optional; aids diagnosis. |
| `line` | integer | `line!()` from the emission callsite. Optional. |
| `pid` | integer | Process ID. Captured at `logging::init()`; stable per `boot`. Optional. |
| `thread` | object | `{id, name}`. `id` is the tokio/thread serial; `name` is `std::thread::current().name()` or `"unnamed-N"`. Optional. |
| `attempt_id` | string \| null | Top-level for direct `jq` query. Promoted from any span in `spans` carrying an `attempt_id`. `null` when no span carries one. Required (null when absent). |
| `spans` | array | Full span stack, outermost-first. Each element `{name, id, attempt_id?}`. Empty array `[]` when emitted outside any span. Required (always present; empty when no span). Codex §2 Finding 1 — fixed v1's singular `span` shape. |
| `msg` | string | The event's `message` field, post-wire-sanitizer (see §5.6). Required. |
| `fields` | object | Structured key/value pairs from the emission callsite, post-redaction. Required (empty `{}` when no fields). |

**JSON encoding rules (Codex §2 Finding 3):**
- Non-finite floats (`NaN`, `+Infinity`, `-Infinity`): encoded as `null` with a sibling marker field `{name}_kind`: `"nan"` \| `"posinf"` \| `"neginf"`.
- Control characters (0x00–0x08, 0x0B–0x1F, 0x7F) in any string field: escaped as `\u00XX` per JSON spec (the `serde_json` default behavior; verified in the formatter unit tests).
- ANSI escape sequences in `msg`: stripped before formatting using the `strip-ansi-escapes` crate. The raw bytes are preserved if needed for trace-level inspection via the field `msg_raw_b64`.
- Field length caps: any string field longer than 4 KB is truncated to 4 KB and suffixed `…[truncated N bytes]` BEFORE encoding. `msg` cap is 8 KB with the same suffix. Byte-preview fields (trace-mode hex dumps) capped at 256 bytes shown (with `bytes_total: N` sibling).
- Top-level event size cap: any event whose post-encoding line would exceed 32 KB is dropped at the Fanout Layer and replaced with a synthetic `event_dropped_oversize` event recording `target`, `seq`, and `original_size_bytes`. The Fanout Layer increments a `events_dropped_total` counter exposed via `logging_status` for visibility.

**Schema evolution rules:**
- `v:1` accepts ADDITION of new optional fields; existing JSONL readers must ignore unknown fields.
- Changing a field's TYPE (e.g., `seq: integer` → `seq: string`) or REMOVING a required field requires bumping to `v:2`.
- Distinction between field-absent and field-`null` is significant: `null` means "the value was explicitly empty"; absent means "the version of tuxlink that wrote this line did not emit the field." Readers MUST treat absent and null distinctly for fields where the spec calls out nullability.

### 3.2 Span and correlation-ID conventions

- **Boot ID** (`boot` field, every event): UUID v7 minted in `tuxlink::logging::init()` at app start; embedded in every emitted event.
- **Spans** (`spans` array): full stack outermost-first. Each entry `{name, id, attempt_id?}`. Always present (`[]` when emitted outside any span). This is the fix for Codex §2 Finding 1 — v1's singular `span` field couldn't hold a B2F-inside-dial nested stack.
- **Attempt ID** (top-level `attempt_id` field): tuxlink convention. Any span representing an operator-meaningful unit of work (a dial attempt, an inbound exchange, a CMS handshake) stamps an opaque short identifier of shape `att-{6-char-base32}` on its span data. The Fanout Layer promotes the innermost-span's `attempt_id` to the top level for direct `jq '.attempt_id == "att-xyz1"'` query. `null` when no span in the stack carries one.

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
| `winlink::session`, `winlink::secure`, `winlink::handshake`, `winlink::telnet*`, `winlink::transfer`, `winlink::wire`, `winlink::lzhuf`, `winlink::telnet_p2p_login` | debug | trace |
| `winlink::modem::ardop`, `winlink::modem::vara`, `winlink::modem::process` | debug | trace |
| `winlink::ax25::frame`, `winlink::ax25::link`, `winlink::ax25::datalink`, `winlink::ax25::kiss`, `winlink::ax25::rfcomm`, `winlink::ax25::params` | debug | trace |
| `winlink::listener::decide`, `winlink::listener::peer`, `winlink::listener::packet_gate`, `winlink::listener::station_password`, `winlink::listener::transport`, `winlink::listener::allowed_stations`, `winlink::listener::arms_record` | debug | trace |
| **Orchestration cluster** (added per Codex §7 Finding 1): `winlink_backend`, `app_backend`, `modem_commands`, `modem_status`, `consent_gate`, `ui_commands`, `compose_window`, `help_window`, `logging_window` | info | debug |
| `winlink::message`, `winlink::proposal`, `winlink::compose`, `native_mailbox`, `winlink::relay_banner` | info | debug |
| `forms::*`, `search::*`, `catalog::*`, `grib::*`, `position::*`, `user_folders` | info | debug |
| `wizard`, `bootstrap`, `config`, `tray`, `theme_state` | info | debug |
| `logging::env_probes::*`, `logging::retention`, `logging::export`, `logging::settings` | info | debug |

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

### 4.4.1 Message-body callsite policy (Codex §7 Finding 2)

`winlink::compose`, `winlink::message`, `winlink::proposal`, `winlink::transfer`, `native_mailbox`, `forms::*`, `catalog::*`, `grib::*` callsites that touch message content (body, subject, headers, attachment bytes) MUST follow this callsite policy:

- **Log:** message ID, callsign(s) involved, byte-size, attachment count + per-attachment size + per-attachment content-type (NOT filename if it could contain operational sensitive info; filename CAN be logged at debug level if needed for parse-error diagnosis), folder, exchange outcome (sent/received/failed), parse errors with error message + offsetinformation.
- **Do NOT log:** message body text, subject text, header field values beyond well-known operational headers (Message-ID, Date, From/To callsigns), attachment binary content, MIME part content, full parsed `message.parts[i].body`.
- **Trace-mode exception:** raw wire bytes for B2F encoding/decoding diagnosis MAY be logged at trace level under the orchestration cluster's debug-only ceiling — but ONLY through the WireSanitizer helper (§5.6), which strips known credential patterns (`;PR:`, `Password:`-response) from the bytes before they reach tracing.

Rationale: the operator's "passwords-only redaction scope" decision (§5.4) does not redact message body, but the original brainstorm explicitly noted body content can be operationally sensitive (ICS-213 to an incident, an EmComm message a third party sees). The callsite policy keeps message content out of logs as a discipline rather than as a redaction-layer feature, eliminating the "accidental MIME body dump" path Codex flagged.

The policy is enforced by code review (per `[[no-incomplete-or-internal-refs-in-shipped-features]]`) and by the integration test in §10.2 that grep-searches the events.jsonl for known message-body markers from the synthetic test corpus.

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

### 5.2 Layer A: field-name blocklist (expanded per Codex §1 Finding 3)

A regex-compiled-once blocklist runs against every field NAME visited by the redacting `Visit` impl. Match → the visited value is replaced with `<redacted>` in the `LoggedEvent` representation BEFORE fanout. The key is preserved so the agent reading the log knows a credential field was present.

```rust
// src-tauri/src/logging/redact.rs

static FIELD_BLOCKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?ix)
        ^(
            # Generic password-class
            password | passwd | pwd | password_input | peer_password
            | station_password | secure_response
            # Token-class
            | token | auth_token | access_token | refresh_token | oauth_token
            | bearer | bearer_token
            | consent_token
            # Secret/key-class
            | secret | client_secret | private_key | privatekey
            | api_key | apikey | api[_-]key
            # Auth-class
            | auth | authorization | auth_header | authheader
            | credential | credentials
            # Challenge/response
            | secure_login_response | secure_login_challenge
            | challenge_response | challenge | response
            # Session/cookie
            | session_cookie | sessioncookie | sessionid | session_id
            | cookie
            # Cryptographic primitives that might carry secret material
            | signature | nonce | hmac | salt
            # Keyring-internal
            | keyring_value | keyring_secret
        )$
    ").expect("redaction blocklist regex must compile")
});

pub fn should_redact_field(name: &str) -> bool {
    FIELD_BLOCKLIST.is_match(name)
}
```

**Anchoring discipline:** the regex is anchored (`^...$`) so plausibly-benign field names (`password_hint_index`, `challenge_round_number`, `nonce_count_total`) do NOT match. Adding a new sensitive key is a one-line change to the regex.

**Documented blocklist false-positive deliberation:** `challenge`, `response`, `nonce`, `signature`, `hmac` are common enough words that they may appear as field names in non-credential contexts. The decision (alpha period): false-positive (over-redact) is preferable to false-negative (leak). If an engineer wants to emit a non-credential `challenge` field (e.g., a UI quiz feature), they rename it (`puzzle_question`, `quiz_text`) or use the structured-field form `tracing::info!(question = %text, "...")` to bypass the blocklist semantically. The blocklist defends against accidental-naming-leaks, not against deliberate-emission.

**Repo-derived test discipline (Codex suggestion):** the blocklist regex test suite includes a `git grep`-derived list of field names actually used in callsites across `src-tauri/src/`, asserting each is correctly classified (block or allow). Re-runnable as new code lands; new credential-shaped names landing fail the test until the regex is updated. The test source is `src-tauri/tests/logging_blocklist_corpus.rs`.

### 5.3 Layer B: custom `Debug` on credential types (source-verified per Codex §1 Finding 4)

Belt-and-suspenders: even if someone writes `tracing::debug!(?creds, "auth state")` (passing the whole struct), the redacting `Visit` calls the struct's `Debug` impl via `record_debug`, which yields `<redacted ...>` not the inner fields.

**Source-verified credential-struct audit list** (grepped against `origin/main` 2026-06-04):

- `winlink::session::ExchangeConfig` — has `password: Option<String>`, currently derives `Debug`. **REQUIRES manual `Debug` impl.** This is the primary leak surface — every CMS dial constructs an `ExchangeConfig`, and any `tracing::debug!(?config, "...")` call leaks the station password without this fix.
- `winlink::listener::station_password::StationPassword` — already has manual `Debug` impl returning `"<redacted StationPassword>"` (line 181). **NO CHANGE NEEDED**; verified existing.
- `winlink::credentials` — no structs; module exposes `read_password(callsign: &str) -> Result<String, KeyringError>` function returning a `String`. The returned `String` is a raw password — callers MUST treat it as a credential. The KeyringError variants (`NoEntry`, `Backend`) carry callsign + error message (no password); their auto-derived `Debug` is safe.
- `winlink::secure` — no structs; module exposes `secure_login_response(challenge: &str, password: &str) -> String` function. Caller responsibility: never `tracing::debug!(?secure_login_response_input, ...)` with a struct that holds the password as a field.

**Tests-and-conventions enforcement (Codex suggestion):** add `src-tauri/tests/credential_debug_audit.rs` that uses compile-time `static_assertions::assert_impl_all!` to verify each named struct in the audit list has a manual `Debug` impl (detected via `impl fmt::Debug for {Struct} { fn fmt }` source presence — checked via a `build.rs` source-scan since `static_assertions` cannot directly check this). When future code adds a struct with a `password: String` field but no manual `Debug`, the build fails with a pointer to this section.

```rust
impl std::fmt::Debug for ExchangeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render every field EXCEPT password; password becomes <redacted>
        f.debug_struct("ExchangeConfig")
            .field("callsign", &self.callsign)
            .field("target", &self.target)
            .field("password", &"<redacted>")
            // ... other fields ...
            .finish()
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

### 5.6 Wire sanitizer — CRITICAL fix per Codex §1 Finding 1

**The leak path:** trace-level wire logging in `winlink::handshake`, `winlink::telnet`, `winlink::telnet_listen`, `winlink::telnet_p2p_login`, and `winlink::transfer` emits raw protocol lines. Source-verified leak sites (`origin/main`):

- `src/winlink/handshake.rs:50`: `out.push_str(&format!(";PR: {response}\r"))` — the 8-digit secure-login token responding to a `;PQ:` challenge. The token is reusable for replay against the operator's account on the time-bounded challenge window.
- `src/winlink/telnet_listen.rs:94`: `WIRE_PROMPT_PASSWORD = b"Password :\r"` — the prompt itself is not sensitive, but the bytes the peer writes IN RESPONSE to the prompt ARE the peer password.
- `src/winlink/telnet_p2p_login.rs` — peer login flow exchanges the peer password in the same shape.
- Any future protocol that puts credential material into wire-line text.

Field-name redaction CANNOT catch these. The credential lives inside the `msg` string (`format!(";PR: {response}\r")`), not in a structured `password` field. Trace-mode export of a dial attempt would contain the operator's reusable secure-login token in plaintext.

**The fix:** a `WireSanitizer` helper that ALL wire-text emissions call BEFORE passing the string to a tracing macro. The sanitizer is a pure function — `pub fn sanitize_wire_line(raw: &str) -> Cow<'_, str>` — that:
1. Pattern-matches the line against known credential-bearing protocols (compiled regex set).
2. Replaces the credential material with `<redacted>` while preserving the surrounding protocol context (so the agent reading the log sees `;PR: <redacted>\r` and knows what kind of line it was, just not the value).
3. Returns `Cow::Borrowed(raw)` when no pattern matched (zero allocation for the common case of clean wire text).

```rust
// src-tauri/src/logging/wire_sanitize.rs

static WIRE_PATTERNS: Lazy<RegexSet> = Lazy::new(|| RegexSet::new(&[
    r"(?i)^;PR:\s*\S+",                        // Secure-login response: ;PR: <digits>
    r"(?i)^;PQ:\s*\S+",                        // Secure-login challenge: ;PQ: <digits> (challenge alone isn't a secret, but redact for parity — defense in depth)
    // Telnet/peer password exchange: when a line follows a "Password :" prompt
    // Sanitize-callsites that emit *response* lines pass a `WireContext::PasswordResponse`
    // tag; the sanitizer treats the entire line as credential.
    r"(?i)^auth\s+\S+\s+\S+",                  // Common "AUTH USER PASS" shapes
]).expect("wire patterns must compile"));

pub enum WireContext {
    /// General protocol-text emission; sanitize via WIRE_PATTERNS only.
    Generic,
    /// The bytes about to be emitted are the response to a "Password :" prompt;
    /// redact the whole line regardless of content.
    PasswordResponse,
    /// The bytes ARE a credential (e.g., what we sent in response to ;PQ).
    Credential,
}

pub fn sanitize_wire_line(raw: &str, ctx: WireContext) -> Cow<'_, str> {
    match ctx {
        WireContext::Credential | WireContext::PasswordResponse => {
            Cow::Owned("<redacted>".into())
        }
        WireContext::Generic => {
            for idx in WIRE_PATTERNS.matches(raw).iter() {
                return Cow::Owned(apply_redaction(raw, idx));
            }
            Cow::Borrowed(raw)
        }
    }
}
```

**Callsite migration:** every wire-text `tracing::trace!(line = %line, "wire tx")` or `tracing::trace!(line = %line, "wire rx")` callsite becomes:

```rust
let sanitized = sanitize_wire_line(&line, WireContext::Generic);
tracing::trace!(line = %sanitized, "wire tx");
```

For the specifically-sensitive sites:

```rust
// In handshake.rs ;PR: emission
let response_line = format!(";PR: {response}\r");
let sanitized = sanitize_wire_line(&response_line, WireContext::Credential);
tracing::trace!(line = %sanitized, "wire tx");
// Then transmit response_line over the wire (unchanged).
```

**Why the WireSanitizer is a helper, not a tracing Layer:** the wire string is constructed BY the callsite. A Layer cannot reach back and mutate the `msg` field after `tracing!` fires (per the immutability constraint that drove the §2.2 architecture redesign). The sanitizer therefore runs at the callsite, BEFORE the tracing macro is invoked. The discipline is enforceable by code-review (and a custom lint, future): any callsite that constructs a wire-text-shaped string and feeds it to a tracing macro must route through `sanitize_wire_line` first.

**Tests:** the WireSanitizer module ships with unit tests covering each known leak pattern + an integration test that walks the full secure-login flow (`handshake.rs::secure_login_response` invocation through the trace-level wire emission), exports the events.jsonl, and asserts the actual 8-digit token bytes do NOT appear anywhere in the archive.

### 5.7 The redacting `tracing::field::Visit` implementation (Codex §1 Finding 5)

The Fanout Layer's redacting visitor implements ALL of `tracing::field::Visit`'s methods, not just `record_str` / `record_debug`. The full set:

```rust
impl tracing::field::Visit for RedactingVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if should_redact_field(field.name()) {
            self.write_field(field.name(), "<redacted>");
        } else {
            // `%value` Display goes through record_str; `?value` Debug goes here.
            // Credential structs with manual Debug impl render <redacted ...> here.
            self.write_field(field.name(), &format!("{value:?}"));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if should_redact_field(field.name()) {
            self.write_field(field.name(), "<redacted>");
        } else {
            self.write_field(field.name(), value);
        }
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        if should_redact_field(field.name()) {
            self.write_field(field.name(), "<redacted>");
        } else {
            // Capture error + full source-chain via `value.source()` walk.
            self.write_field(field.name(), &render_error_chain(value));
        }
    }
    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        if should_redact_field(field.name()) {
            self.write_field(field.name(), "<redacted>");
        } else {
            // Cap at 256 bytes preview + record full length.
            let preview = &value[..value.len().min(256)];
            self.write_field(
                field.name(),
                &format!("{} bytes; preview: {}", value.len(), hex::encode(preview)),
            );
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) { /* numeric handling */ }
    fn record_u64(&mut self, field: &Field, value: u64) { /* numeric handling */ }
    fn record_i128(&mut self, field: &Field, value: i128) { /* numeric handling */ }
    fn record_u128(&mut self, field: &Field, value: u128) { /* numeric handling */ }
    fn record_bool(&mut self, field: &Field, value: bool) { /* numeric handling */ }
    fn record_f64(&mut self, field: &Field, value: f64) {
        if value.is_finite() {
            self.write_field(field.name(), &value.to_string());
        } else {
            // NaN/Inf encoding rule per §3.1
            self.write_field(field.name(), "null");
            self.write_field_marker(format!("{}_kind", field.name()), classify_nonfinite(value));
        }
    }
}
```

**Nested-container policy (Codex §1 Finding 6):** containers passed as field values (`serde_json::Value`, `HashMap<String, _>`, custom DTOs serialized through Serde) bypass the field-name visitor since the visitor sees only the top-level field name. Two enforced rules prevent this leak:

1. **Lint discipline:** `tracing::*!(payload = ?some_json_value, "...")` patterns are flagged by a custom clippy lint (initially a `git grep` test in `src-tauri/tests/no_opaque_container_emissions.rs` that fails the build if a callsite emits a `serde_json::Value`, `HashMap<String, _>`, or any module-defined DTO that has `#[derive(Serialize)]` as a tracing field).
2. **For places where structured payload IS the intent** (env-probe output rendering, manifest construction): the payload is constructed THEN passed through a recursive-redaction pre-pass (`tuxlink::logging::redact::redact_json_recursive(&mut value)`) BEFORE emission. The recursive pass applies the field-name blocklist to nested JSON object keys at any depth.

The lint+recursive-redact pair eliminates the path Codex flagged where env-probe outputs could ship a `password` key buried in a 3-level-nested probe-result struct.

### 5.8 Required tests (expanded per Codex §11 Finding 1)

**Unit (`src-tauri/src/logging/redact.rs`):**
- Every name in the §5.2 blocklist matches the regex.
- Control cases: `password_hint_index`, `challenge_round_number`, `nonce_count_total`, `key_event_handler`, `cookie_jar_path` do NOT match.
- `should_redact_field` is case-insensitive (already by regex flag, but tested).

**Unit (`src-tauri/src/logging/wire_sanitize.rs`):**
- `;PR: 12345678` → `;PR: <redacted>`.
- `;PQ: 12345678` → `;PQ: <redacted>` (defense in depth).
- Generic wire text without pattern match: passes through unchanged (Cow::Borrowed asserted).
- `WireContext::Credential` always returns `<redacted>` regardless of input.
- `WireContext::PasswordResponse` always returns `<redacted>` regardless of input.

**Unit (`src-tauri/tests/credential_debug_audit.rs`):**
- `ExchangeConfig`'s `Debug` impl returns string containing `"password": "<redacted>"` and NOT containing inner password value.
- `StationPassword`'s `Debug` impl returns string starting with `"<redacted"`.

**Unit (`src-tauri/tests/logging_blocklist_corpus.rs`):**
- Repo-derived field-name corpus: every tracing-callsite field name is asserted to be either explicitly allowed or correctly blocked. New code adding a credential-shaped field name must update either the regex or the test allowlist.

**Integration (`src-tauri/tests/redaction_integration.rs`):**
- `tracing::debug!(password = %real_pw, ...)` → events.jsonl contains `"password":"<redacted>"`, does NOT contain `real_pw`.
- `tracing::debug!(?creds, ...)` where `creds: ExchangeConfig` carrying a password → events.jsonl contains `"<redacted>"` for password field, does NOT contain inner password value.
- `tracing::debug!(token = %real_token, ...)` (Display via `%`) → events.jsonl contains `"token":"<redacted>"`.
- `tracing::error!(error = &err as &dyn Error, ...)` where `err` chains include a credential value as a sub-error message → currently emits the error chain; **the `error` field-name is in the blocklist** so the entire chain is redacted. (Acceptable tradeoff: error message text containing accidental credentials cannot be distinguished from legitimate error context; redact-the-whole-chain is safer.)
- `tracing::debug!(payload = ?json_with_nested_password, ...)` → the no-opaque-container lint fails the build at this site.
- `tracing::debug!(byte_dump = &raw_bytes[..], ...)` where the bytes contain known password material → the bytes-field handler caps preview at 256 bytes hex-encoded; the test asserts the password bytes are NOT in the hex preview (sensitive bytes test corpus is generated from synthetic credential strings).

**Wire-sanitizer integration test (the CRITICAL gate):**
- Synthesize a full secure-login flow: construct `ExchangeConfig` with password `"hunter2hunter2"`, drive `handshake.rs` through a fake socket that emits a `;PQ:` challenge, allow the response code path to construct the `;PR: {response}\r` line, run the export pipeline, decompress the archive, `grep -a` the events.jsonl for the actual 8-digit token bytes computed for `("23753528", "hunter2hunter2")` → assert NOT FOUND.
- Same test for telnet password-response flow.
- Same test for peer P2P login flow.

**Smoke for end-to-end "no secret bytes":** the `tuxlink-logging-smoke.sh` script (§10.4) runs an end-to-end synthetic auth flow with a known credential value, exports, and greps the archive's decompressed events.jsonl for the credential. Asserts NOT FOUND. Exits non-zero if found.

---

## 6. Storage

### 6.1 Location and resolution fallbacks (Codex §10 Finding 2)

Logs live at `$XDG_STATE_HOME/tuxlink/logs/` (typically `~/.local/state/tuxlink/logs/`). State_home is correct per the XDG Base Directory Specification — logs are state, not cache; they survive `rm -rf ~/.cache` and cache-clearing tools.

**Resolution order** (`tuxlink::logging::state_dir::resolve()`):
1. `$XDG_STATE_HOME` if set and absolute.
2. `$HOME/.local/state/tuxlink/logs/` if `$HOME` is set.
3. **Fail soft** with a UI-visible error state surfaced via `logging_status` Tauri command and a `warn`-level event written to the temporary stderr subscriber (since the durable subscriber cannot start without a log directory). The Logging window displays "Log directory unavailable: <reason>; tuxlink will run without disk logging."

**Edge cases handled:**
- **Sudo / root invocation**: detected via `nix::unistd::geteuid()`. Logs go to `/root/.local/state/tuxlink/logs/` (per XDG semantics, root's home). A `warn`-level event records `running_as_root: true` so the agent diagnosing knows logs aren't where the operator expected.
- **Flatpak / sandboxed home**: `dirs::state_dir()` returns the sandboxed path; no special handling needed. Manifest records the resolved path so the agent knows.
- **Unset `$HOME`**: extremely rare on Linux; treated as fail-soft (see resolution step 3).
- **Permission denied on directory create**: fail-soft. The Logging window surfaces the underlying I/O error.

The **resolved log directory** is recorded in `manifest.json` under `logging.log_dir` so an exported archive carries this metadata.

### 6.2 Rolling strategy and filename convention (Codex §12 Finding 2 — consistency fix)

`tracing-appender::rolling::Builder` with `Rotation::HOURLY` and UTC-midnight-aligned hour boundaries:

```rust
let file_appender = tracing_appender::rolling::Builder::new()
    .rotation(Rotation::HOURLY)
    .filename_prefix("tuxlink")
    .filename_suffix("jsonl")
    .build(state_home.join("tuxlink/logs/"))
    .map_err(LoggingInitError::AppenderBuild)?;

let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);
```

**Filename convention (canonical):** `tuxlink.YYYY-MM-DD-HH.jsonl`. The `tracing-appender` library uses the `prefix.{date}.suffix` shape with `.`-separators between prefix/date/suffix and `-`-separators within the date itself. Every reference in this spec (sweep glob, file enumeration, export reader) uses this exact pattern. The retention sweep glob is `tuxlink.*.jsonl`, not `tuxlink-*.jsonl` (v1's error per Codex).

Properties:
- **Hourly rotation** at UTC hour boundaries.
- **Non-blocking writes** — events queue to a background thread; emission callsites never block on disk I/O.
- **Atomic per-event writes** — each line is a single write call; partial writes do not happen.
- **Crash-resistant** — unflushed events in the buffer (last ~1s of activity) are lost on hard crash; no file corruption.

**File-system safety (Codex §13 Finding 1):**
- The log directory is created with mode `0700` (owner read/write/execute only).
- Each log file is created with mode `0600` (owner read/write only).
- Before writing, `tuxlink::logging::state_dir::resolve()` validates that the resolved path is NOT a symlink (`std::fs::symlink_metadata` + `FileType::is_symlink`); refuses to operate via symlink. Rationale: same-user attacker or compromised process redirecting logs.
- The path is canonicalized via `std::fs::canonicalize` AFTER directory creation; the canonical path is asserted to be under `$XDG_STATE_HOME` or `$HOME`. Refuses any path that escapes via `..` or symlink-on-an-intermediate-component.

### 6.3 Active-file protection in retention sweep (Codex §8 Finding 2)

`tuxlink::logging::retention::sweep` runs:
- At startup, BEFORE `logging::init()` opens the appender (sweeps any leftover files from previous runs).
- After each hour rotation (the disk-consumer task signals the sweeper via a channel after writing each event whose timestamp's hour differs from the previous event's).
- Immediately when the operator changes retention values via the Logging window.

**Active-file rule (CRITICAL fix):** the sweeper NEVER deletes the file currently held open by the appender. The `LoggingState` exposes `active_file_path() -> PathBuf`, which the sweeper consults on every pass. The active path is excluded from the sweep set unconditionally.

Sweep logic:
1. List `tuxlink.*.jsonl` files in the log directory.
2. EXCLUDE the active path.
3. Sort closed files by filename (which sorts by UTC hour; the date prefix has fixed positional width).
4. Compute current total size (active file size + closed files size).
5. Determine cutoff via two rules, take the more aggressive:
   - **Days rule**: closed files older than `retention_days` days → mark for delete.
   - **Size rule**: if total size exceeds `retention_mb_cap`, mark oldest closed files for delete one at a time until under cap.
6. Delete marked files. Emit `retention sweep: deleted N files (X MB), retained Yd Zh / W MB, active path X` at `info`.

**Clock-backward grace (Codex §10 Finding 3):** the sweep's days-rule comparison uses BOTH the filename-parsed UTC and the file's mtime; the file is "older than N days" only when BOTH comparisons agree. If they disagree by more than 1 hour (clock skew suspected), the file is NOT deleted and a `warn`-level event records the disagreement. This protects against NTP-corrected-backward time deleting fresh logs.

Retention bounds: `retention_days` 1–365; `retention_mb_cap` 50 MB – 10 GB.

### 6.4 Disk-error handling (Codex §10 Finding 1)

The free-disk guard handles the SLOW-developing low-disk case (polling-based). The FAST-developing case (ENOSPC, quota exhaustion, EIO, inode exhaustion) requires the appender's write errors to be observed.

**Appender error observation:** `tracing-appender::non_blocking` returns a `WorkerGuard` and an internal error count. The disk-consumer task polls `WorkerGuard::error_counter()` every 30 seconds. When the counter increments since the last poll, the consumer emits a `warn`-level event `disk-write-error: N writes failed since last poll` AND triggers a free-space probe (which then emits the warn event with concrete free-space data if low). 

Belt-and-suspenders for the rare case where appender errors and free-space-low don't coincide (e.g., per-user quota with plenty of free disk): the disk-consumer also wraps each tracing-appender write attempt with its own error tracking and emits per-error degradation events.

**Free-disk guard logic** (refined):
- When `$XDG_STATE_HOME`'s filesystem reports less than 100 MB free OR per-user disk quota reports less than 100 MB available:
  1. Emit `warn`-level `disk-space-low: stopping log writes; free=X MB`.
  2. The disk consumer pauses queueing new events to the appender. Events still flow to the UI subscriber and are still broadcast through the Fanout Layer.
  3. Re-check every 5 minutes; resume when free space recovers above 200 MB. Resume emits `info`-level `disk-space-recovered: resuming log writes`.

### 6.5 Concurrency model (revised per Codex §8 Findings 2-4)

- **Writer**: tracing-appender's non-blocking appender owns the file handle. Single writer thread; lock-free MPMC queue feeds it from the disk-consumer task.
- **Reader (export pipeline)**: opens each closed `tuxlink.*.jsonl` file read-only. For the ACTIVE file:
  1. The export pipeline signals a flush barrier via `LoggingState::flush_barrier()`. The Fanout Layer awaits the broadcast channel draining + the disk-consumer's write queue draining. Bounded wait (default 500ms) — if the barrier times out, export proceeds with a `warn` recording the timeout.
  2. The reader then opens the active file at the current size + reads up to that size; events arriving DURING the read after the barrier are excluded from the export (and remain durably on disk for next export).
  3. Reader handles the case where the file's last line is mid-write (a partial JSON line at EOF) by detecting unterminated JSON and skipping that line; a `warn`-level event records the skip.
- **Retention sweeper**: never touches the active file (per §6.3). Sweep operates on closed files only; the active file lock is implicit (the writer never closes it during a normal hour).
- **Detailed-mode auto-revert** (Codex §8 Finding 4): the auto-revert timer triggers a single `LoggingState::set_detailed_mode(Off)` call. The filter-layer state is wrapped in `tracing_subscriber::reload::Handle<EnvFilter, S>`; the reload is atomic across all subscribers. Mid-emission events during the reload may use either the pre- or post-reload filter (acceptable; events stay correctly leveled per their source target). Free-disk stop/resume uses the same reload mechanism for the disk-consumer's pause-state flag, atomic across the consumer task.

---

## 7. Compression

### 7.1 v0 strategy (revised per Codex §3 Finding 1)

- **Outer tarball** (`tuxlink-logs-*.tar.zst`): zstd level 19 (was: 22 with `--long=27`). Long-range mode dropped from the outer archive to ensure standard `zstd -d` decompresses without requiring `--long` on the decompression side. Older zstd builds (pre-1.4.0, ~2019) or memory-constrained systems can reject long-window frames; the outer archive is the recipient-side decompression and must work on stock distros without flag-fiddling.
- **Inner events stream** (`events.jsonl.zst` inside the tarball): zstd level 19 with the bundled dictionary. Dictionary embedded in the archive as `dict.zdict`. Decompresses with stock `zstd -d -D dict.zdict events.jsonl.zst` on any system with zstd ≥ 1.4.0.
- **Documented zstd version requirement:** zstd ≥ 1.4.0 (released 2019-04). The agent's machine and the alpha tester's machine both need this. The smoke script (§10.4) checks `zstd --version` and warns if below threshold.

### 7.2 v0 dictionary

`src-tauri/assets/logging/tuxlink-events-v1.zdict` (~16 KB), trained from a synthetic corpus generated by the `xtask` `gen-corpus` binary. Dictionary is bundled into the tuxlink binary via `include_bytes!`. The same dictionary is shipped inside every export archive (as `dict.zdict`) so archives are self-decompressing.

### 7.3 Synthetic corpus (v0 training input) — expanded per Codex §5 Finding 1

`xtask/src/bin/gen-corpus.rs` produces approximately 1.5–2 MB of representative JSONL events at `dev/log-corpus-synthetic/` (gitignored). Coverage:

**Protocol-level event sequences:**
- Dial attempt event sequences across all three transports (telnet, ARDOP, VARA)
- B2F handshake event sequences
- Modem command/response exchanges (ARDOP and VARA)
- AX.25 frame events (SABM, UA, I-frame, RR, DISC)
- Listener inbound-session events

**Real-string fixtures (added per Codex Finding 1):**
- Captured stderr fixtures from `gnome-keyring-daemon`, `kwallet`, `KeePassXC` with realistic error message variants
- Captured Flatpak portal / xdg-desktop-portal error messages
- PipeWire / ALSA command stderr (sample-rate mismatch, device-busy, ENODEV)
- VARA stderr quirks (licensing, busy, PTT, modem-state-machine errors)
- ARDOP `FAULT` / `BUFFER` quirks and command echoes
- BlueZ / RFCOMM errors (org.bluez.Error.* variants)
- systemd-journal-formatted lines for environment probes that shell out
- Locale-influenced messages (en_US.UTF-8 default; locale variants where applicable)

**Environment probe outputs:**
- All six environment probe outputs with realistic value variation
- Real D-Bus reply shapes for Secret Service collection queries
- Real `/dev/serial/by-id` listing strings with vendor prefixes (`usb-DigiRig_DigiRig_LITE_*`)
- Real `wlrctl toplevel list` output shapes

**Boilerplate:**
- Wizard / bootstrap / config events
- Error variations across each subsystem (timeouts, refused, malformed, unauthorized)

Variation includes multiple callsigns, multiple correlation IDs, multiple gateway names, multiple frequency values, multiple error message strings, multiple timestamp distributions.

The generator's source data is committed to `dev/log-corpus-fixtures/` (NOT gitignored — operator-curated reference material kept small). The generator combines fixtures with templated values to produce the final corpus.

### 7.4 v1 dictionary upgrade path

When alpha collects ~5–10 MB of real-corpus data over a few weeks:
1. Operator runs `cargo run --manifest-path xtask/Cargo.toml --target-dir xtask/target --bin train-log-dict -- --input dev/log-corpus-real --output src-tauri/assets/logging/tuxlink-events-v2.zdict --size-kb 32`.
2. Bump the `include_bytes!` filename in `src-tauri/src/logging/dict.rs` to `v2`.
3. Bump `dict_version` constant.
4. Ship.

The infrastructure code does not change. v1 is a single asset add + three-line code change.

### 7.5 Dictionary validation and fallback (clarified per Codex §3 Finding 3)

Two distinct failure modes; clarification of where each is handled:

**Build-time:** the `tuxlink-events-v1.zdict` asset is loaded via `include_bytes!`. If the file is missing at the source path, the build fails. This is enforced at compile time, not run time.

**Export-time validation (clarified per plan-adrev §1 Finding "Dictionary validation is claimed but not actually possible via this call"):** when the export pipeline initializes, it calls `dict::load_validated() -> Result<&[u8], DictError>`. The validation actually exercises the dictionary via a known-input roundtrip (NOT via `DecoderDictionary::copy`, which does not return a `Result`):

```rust
pub fn load_validated() -> Result<&'static [u8], DictError> {
    VALIDATED.get_or_init(|| {
        if EVENT_DICT_V1.is_empty() {
            return Err(DictError::Empty);
        }
        // Real validation: compress a known input WITH the dictionary, then
        // decompress WITH the same dictionary, assert roundtrip equality.
        const PROBE: &[u8] = b"tuxlink-dict-validation-probe-2026";
        let compressed = zstd::stream::Encoder::with_dictionary(Vec::new(), 1, EVENT_DICT_V1)
            .and_then(|mut e| { e.write_all(PROBE)?; e.finish() })
            .map_err(|e| DictError::Invalid(format!("compress: {e}")))?;
        let decompressed = zstd::stream::Decoder::with_dictionary(compressed.as_slice(), EVENT_DICT_V1)
            .and_then(|mut d| { let mut out = Vec::new(); d.read_to_end(&mut out)?; Ok(out) })
            .map_err(|e| DictError::Invalid(format!("decompress: {e}")))?;
        if decompressed != PROBE {
            return Err(DictError::Invalid("roundtrip mismatch".into()));
        }
        Ok(EVENT_DICT_V1)
    }).clone()
}
```

On error: the export pipeline catches the error, emits a `warn`-level `dict-invalid: falling back to dict-free compression` event with the underlying error message, then proceeds without dictionary. `manifest.json` records `inner_dict_version: null` for this archive.

The validation runs once at process start and the result is cached in a `OnceCell`. Subsequent exports reuse the cached validation result.

**Compression-ratio telemetry (added per Codex §5 Finding 2):** every export records in `manifest.json`:
- `compression.raw_events_bytes` — uncompressed events.jsonl size (post-redaction)
- `compression.inner_compressed_bytes` — size of events.jsonl.zst inside the tarball
- `compression.outer_archive_bytes` — final .tar.zst size on disk
- `compression.inner_ratio` — `raw_events_bytes / inner_compressed_bytes` (rounded to 2 decimal places)
- `compression.dict_amortized_ratio` — `raw_events_bytes / (inner_compressed_bytes + dict_size_bytes)` (the realistic per-archive ratio with the dictionary's bytes accounted)

Telemetry lets us (and the agent) judge whether v0 dictionary is delivering value. If ratios are systematically low (dict_amortized_ratio < 1.5 for typical archives), retraining is justified. The acceptance criteria (§10) include "smoke compares with-dict vs no-dict on the synthetic corpus and asserts dict_amortized_ratio > 1.3" so we have a baseline.

### 7.6 Outer tarball normalization (Codex §3 Finding 2)

Tar archive member metadata is normalized for portability + reproducibility:
- All member names are fixed and relative: `summary.txt`, `events.jsonl.zst`, `dict.zdict`, `manifest.json`. No directory members. No `..` components.
- All members have mode `0600` (owner read/write; no group/other access).
- All members have uid=0, gid=0, owner_name=empty, group_name=empty.
- All members have mtime = the `exported_at` timestamp (deterministic across runs of the same export).
- The tar uses Posix-ustar format (not GNU long-name extensions; no member name exceeds 100 chars).

Implementation uses the `tar` crate's `HeaderBuilder` with explicit settings, not the convenience `Builder::append_path` (which inherits filesystem metadata).

### 7.7 Decompression at the agent's end

```bash
zstd -d tuxlink-logs-XXX.tar.zst -o tuxlink-logs-XXX.tar
tar xf tuxlink-logs-XXX.tar
# yields: summary.txt, events.jsonl.zst, dict.zdict, manifest.json
zstd -d -D dict.zdict events.jsonl.zst -o events.jsonl
jq '.target' events.jsonl | sort -u   # see which clusters emitted
```

Requires only stock `tar` and `zstd` ≥ 1.4.0. No tuxlink-specific tools.

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

### 8.5 Report Issue flow (failure-path-aware per Codex §9 Finding 3, §13 Finding 4)

`Help → Report Issue` triggers `report_issue_flow`:

1. **Invokes `logging_export`** to produce the archive. Save As dialog opens; operator chooses location.
   - **If Save As is canceled:** the whole flow aborts. Modal shows "Report Issue canceled — no archive produced." No browser open.
   - **If Save As fails (permission denied, disk full):** modal shows the error + offers Copy-template-to-clipboard fallback. Browser does NOT open until export succeeds.
2. **On successful export, the GitHub URL is constructed and length-checked:**
   - GitHub's URL length limit is approximately 8 KB. The body template + substitution typically fits under 1 KB.
   - The substituted body is checked against a 6 KB safety threshold. If the body would exceed 6 KB (extreme correlation-ID, very long file path, etc.), the body is shortened: drop the verbose section headers and keep the metadata block + correlation ID + a single "see attached" placeholder.
3. **Browser open via `tauri-plugin-shell::open`:**
   - **Success path:** browser opens to the pre-filled URL. Modal in main window shows "Log export saved to {path} · Opened GitHub Issues in your browser." with `Copy archive path` + `Open browser` (re-open) buttons.
   - **No browser configured / open fails:** modal shows the URL itself in a selectable text area + `Copy URL` + `Copy archive path` + `Copy issue body to clipboard` buttons. Operator pastes URL into the browser themselves.
   - **The clipboard fallback always exists**, regardless of browser success — the modal's `Copy issue body to clipboard` button is always present so an operator who can't / doesn't want to drag the file can paste the body + manually attach.

4. **Body template substitution with Markdown-safe escaping (per Codex §13 Finding 4):**

   The runtime values substituted into the template are FIRST escaped for safe Markdown rendering inside backticks, then URL-encoded for the URL. The escape function:
   - Replaces backticks in substituted values with `&#96;` (HTML entity rendered as backtick by GitHub but doesn't break the code-span)
   - Replaces newlines in substituted values with `\\n` literal sequence
   - Strips ANSI escapes
   - Caps each substituted value at 500 bytes (path values shouldn't exceed; if they do, truncate with `…[truncated]` marker)

   Values substituted: `{version}`, `{git_sha}`, `{os}`, `{kernel}`, `{correlation_id}`, `{exported_at}`, `{archive_path}`, `{archive_size}`. Note: `{archive_path}` is the only substituted value that's USER-INFLUENCED (operator chose the Save As location); the others come from build/runtime metadata that we control.

5. **The body template:**

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

### 8.6 GitHub issue template file

A new `.github/ISSUE_TEMPLATE/bug.md` file in the repo carries the same template structure so users who reach GitHub directly (not via the Tuxlink button) see the same shape.

### 8.7 Single source of truth for export

Both `Help → Logging → Export logs…` and `Help → Report Issue` invoke the SAME `tuxlink::logging::export::build_archive()` function. There is no second code path. A bug in the export pipeline surfaces identically through both menu items — fewer surfaces to QA.

### 8.8 Env-probe UI subscription (per Codex §9 Finding 4)

The Logging window's Environment-probes section subscribes via Tauri events, not polling:

- Backend emits `logging://probes/snapshot-updated` event with the latest `EnvProbesSnapshot` payload whenever:
  - The startup snapshot completes.
  - An on-error probe run completes.
  - Operator triggers `logging_env_probes_rerun` from the window.
- The Logging window's `useEffect` subscribes via `listen('logging://probes/snapshot-updated', ...)` on mount and updates local React state.
- A "Last updated: HH:MM:SS UTC" timestamp is always visible in the section so the operator can see freshness (Codex's "stale green status" risk).
- On window close, the listener is cleaned up via the returned `UnlistenFn`.

This eliminates the polling-vs-push ambiguity v1 had.

---

## 9. Environment probes — hard alpha requirement

Per operator direction this session: probe coverage is a release-gate criterion, not nice-to-have. The reasoning: without proactive environmental capture, an alpha tester with a broken keyring (or audio device, or serial port) gets logs containing the SYMPTOM but not the CAUSE. The diagnosing agent's next step becomes "send me the output of `systemctl --user status gnome-keyring-daemon` and `echo $DBUS_SESSION_BUS_ADDRESS`" — a multi-round back-and-forth with someone who lacks the Linux skill to answer.

### 9.1 RADIO-1 contract (per Codex §14 Finding 1) — MANDATORY

**Every probe is read-only with respect to the live radio.** RADIO-1 forbids automation, tests, or subagents from initiating transmissions or modifying live modem/listener state without per-invocation licensee consent (CLAUDE.md §"Live radio network operations" + `docs/live-cms-testing-policy.md`).

The probe contract:
- **PERMITTED operations:** DNS resolution, TCP socket open + immediate close (no protocol exchange), HTTP GET to public reachability endpoints (e.g., a non-protocol HTTP service), filesystem stat / read, D-Bus introspection method calls that return metadata, `systemctl --user is-active` (read-only state query), `ps`/`procfs` reads of named processes, hardware device enumeration via sysfs / udev, environment-variable reads (allowlisted names only).
- **FORBIDDEN operations:** CMS authentication (no `;PR:`, no `;PQ:` exchange), Winlink B2F protocol initiation, VARA `CONNECT` command, ARDOP `CONNECT` / `ARQCALL` command, KISS-frame transmission, PTT keying via any path (GPIO, CAT, hamlib, modem-relay), serial-port `write()` to a configured radio device, any modem state-changing command (BANDWIDTH, MYCALL, AUXCALL change, listener arm/disarm).
- **CMS reachability probe specifically:** must NOT do a Winlink-protocol login. Permitted: DNS resolve `cms-z.winlink.org`, TCP-connect to port 8772/8773 and IMMEDIATELY close (no banner read, no command send). The probe records "TCP-connect succeeded" not "CMS reachable" — the distinction matters because the latter implies the protocol is operational.
- **Modem-process probe specifically:** must NOT spawn or send commands to VARA/ARDOP. Permitted: `ps` / `procfs` read to detect running processes, read the spawn args / exit code / stderr tail from cached state maintained by the SESSION code (the session code already tracks process lifecycle; the probe queries that cache, not the process directly).

**Test enforcement:** the probe modules have unit tests using `#[deny(...)]` attribute combined with a compile-fail test (`tests/probes_no_tx_apis.rs`) that asserts the probe modules do NOT import `winlink::session`, `winlink::modem::ardop::command`, `winlink::modem::vara::commands`, `winlink::secure`, `winlink::handshake`, `winlink::transfer`. The static-imports check fails the build if a probe gains a dependency on a TX-touching module.

### 9.2 Probe trigger pattern (debounced + single-flight per Codex §4 Finding 3)

Each probe runs:
- **After app's first paint** (NOT during startup synchronously — Codex §4 Finding 4): the main window emits a `first_paint_complete` Tauri event after React's first useEffect tick. The probe-runner subscribes to this event and dispatches a startup probe pass. Cold-start UI latency is preserved.
- **On error events from its subsystem** with **debounce + single-flight**:
  - Each probe maintains atomic state: `Idle` → `Pending` → `Running` → `Idle` (after run + cooldown).
  - When an error event tagged with the probe's subsystem fires, the runner attempts CAS `Idle → Pending`. If successful, runs the probe. If state is `Running` or `Pending`, increments a `coalesced_count` and returns immediately.
  - After a probe completes, the state transitions to `Idle` only after a 60-second cooldown timer expires. Subsequent errors within the cooldown window do NOT re-run the probe (preventing probe storms when a modem is dying and emitting errors continuously).
  - The probe's emitted output includes `coalesced_count` showing how many errors triggered while running.
- **Manual trigger from Logging window** (`logging_env_probes_rerun`): bypasses the cooldown (operator-initiated).

### 9.3 The six v0 probes (expanded coverage per Codex §4 Finding 1)

| Probe | `target` | Captures |
|---|---|---|
| **keyring** | `tuxlink::logging::env_probes::keyring` | Compile features (which keyring backend feature flags are active); D-Bus session-bus address presence + reachability via `org.freedesktop.DBus.Introspectable`; XDG_RUNTIME_DIR existence + perms; backend-detection-via-D-Bus (Secret Service collection owner: gnome-keyring vs kwallet vs KeePassXC; cross-checked with `systemctl --user is-active gnome-keyring-daemon` / `kwallet5` / `keepassxc` for cached/stale-process distinction); `~/.local/share/keyrings/` existence + owner-uid match (gnome-keyring path); `~/.local/share/kwalletd/` existence (kwallet path); default collection lock state (D-Bus `org.freedesktop.Secret.Service.GetSession` reachability probe); tuxlink entries count (`org.freedesktop.Secret.Collection.SearchItems` count for service=tuxlink); Flatpak portal detection (`org.freedesktop.portal.Secret` presence). |
| **audio** | `tuxlink::logging::env_probes::audio` | PipeWire reachability (`pw-cli info 0` exit code + parse); ALSA reachability (`aplay -l` parse); active source/sink list (`pactl list short sinks` / `sources` when PipeWire's pulse compat is on); configured tuxlink audio device match by name OR alsa hw:N pattern; sample-rate support (`pactl list sinks | grep "Sample Specification"`); DigiRig detection (`pactl list short cards | grep -i digirig`); separately notes whether VARA-managed audio is "external" (VARA owns the device; tuxlink sees the device as in-use) versus ARDOP-managed audio (where ARDOP is a child process tuxlink spawned). |
| **serial** | `tuxlink::logging::env_probes::serial` | `/dev/serial/by-id` listing with vendor/model strings; `/dev/ttyACM*` and `/dev/ttyUSB*` listing as fallback; configured tuxlink serial port existence; permissions (mode + owner uid + gid + match to `dialout` group); user `dialout` group membership (`getgroups` + `getgrnam`); KISS-transport-specific: TCP host:port reachability (TCP-connect close) for configured KISS-TCP, Bluetooth RFCOMM `/dev/rfcomm*` listing, configured Bluetooth MAC reachability via `hcitool name` (or `bluetoothctl info`). |
| **modem_process** | `tuxlink::logging::env_probes::modem_process` | VARA / ARDOP process state from CACHED runtime state maintained by `winlink::modem::process` (the probe READS this cache; it does NOT spawn or query the modems). Cached state includes: last spawn args, last exit code + signal, stderr tail (256 bytes), uptime if running, last-known modem mode (if any). Plus `ps` / `procfs` enumeration of running processes by name to detect "modem running but tuxlink doesn't know about it" inconsistency. |
| **network** | `tuxlink::logging::env_probes::network` | DNS resolution for `cms-z.winlink.org`, `cms-c.winlink.org`, `cms-vt.winlink.org` (multi-CMS); IP route to each CMS IP (parsed `ip route get` output); TCP-connect-and-close to ports 8772, 8773 (no protocol); cached `last_successful_cms_contact_at` from a NEW runtime state (`tuxlink::cms_health::CmsHealthState` — **top-level module at `src-tauri/src/cms_health.rs`, NOT under `winlink::session::*`**, to keep probes RADIO-1-isolated per §9.1) updated by actual CMS session code on success/failure events. Per Codex §4 Finding 5 + plan-adrev §5.1 (probe-isolation-conflicts-with-cms_health-placement): top-level module placement resolves the apparent conflict between §9.1's "probes never import winlink::session::*" rule and §9.2's "network probe reads CmsHealthState." |
| **display** | `tuxlink::logging::env_probes::display` | `WAYLAND_DISPLAY` / `DISPLAY` presence; WebKitGTK version via `webkitgtk` package query or runtime; GPU vendor string via `glxinfo` or `eglinfo` parse; display server detected (sway / labwc / GNOME / KDE via `wmctrl -m` or process-name); `wlrctl toplevel list` reachability for the wayland-tool probe ecosystem the project's `linux-desktop-integration-validation` memory uses. |

### 9.4 Probe-specific redaction + env-var allowlist (expanded per Codex §4 Finding 2)

Probes use an **explicit allowlist of environment-variable NAMES** they read. Probes NEVER dump `std::env::vars()`. The allowlist:

```rust
// src-tauri/src/logging/env_probes/mod.rs
const ENV_ALLOWLIST: &[&str] = &[
    // XDG family
    "XDG_RUNTIME_DIR", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME",
    "XDG_CACHE_HOME", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "XDG_SESSION_DESKTOP",
    // D-Bus
    "DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS",
    // Desktop
    "DESKTOP_SESSION", "WAYLAND_DISPLAY", "DISPLAY", "WAYLAND_SOCKET",
    // User
    "HOME", "USER", "LOGNAME",
    // Locale
    "LANG", "LC_ALL", "LC_CTYPE", "LC_MESSAGES", "LC_COLLATE",
    // Diagnostic basics
    "PATH", "PWD", "SHELL", "TERM", "TERM_PROGRAM", "COLORTERM",
    // Tuxlink overrides (documented in CLAUDE.md / pitfalls)
    "TUXLINK_CONFIG_DIR", "TUXLINK_CMS_HOST", "TUXLINK_CMS_PORT", "TUXLINK_CMS_PLAINTEXT",
    "TUXLINK_GPSD_ADDR", "TUXLINK_VARA_TCP_HOST", "TUXLINK_VARA_TCP_PORT",
    "TUXLINK_ARDOP_TCP_HOST", "TUXLINK_ARDOP_TCP_PORT",
];

// Plus exclusion regex applied DEFENSIVELY even within the allowlist:
static ENV_VALUE_EXCLUSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(password|token|secret|key|auth|bearer|credential)").unwrap()
});

// Allowlist match + exclusion check at value emission:
fn safe_env_value(name: &str) -> Option<String> {
    if !ENV_ALLOWLIST.contains(&name) { return None; }
    let val = std::env::var(name).ok()?;
    if ENV_VALUE_EXCLUSION.is_match(name) || ENV_VALUE_EXCLUSION.is_match(&val) {
        return Some("<redacted>".into());
    }
    // PATH-like values: truncate to 500 bytes
    if val.len() > 500 { return Some(format!("{}…[truncated]", &val[..500])); }
    Some(val)
}
```

PATH-like values get truncated. Any allowlisted variable whose name OR value matches the exclusion regex is redacted (belt-and-suspenders — `LDAP_AUTH=...` would somehow get allowlisted? — redact it).

### 9.5 Probe ordering and cost (per Codex §4 Finding 4)

- **Startup probes:** fire AFTER first-paint (`first_paint_complete` Tauri event from frontend). Not blocking. Run sequentially in a tokio task with per-probe deadlines (default 2s each; total startup probe pass ≤12s under adversarial slowness).
- **Per-probe deadline:** each probe wraps its top-level work in `tokio::time::timeout(per_probe_deadline)`. On timeout, emits a `probe-timeout` `warn`-level event with `partial_result: <whatever was collected so far>` and skips the rest.
- **Per-command deadline:** within a probe, each shell-out / system call has its own deadline (default 500ms). Slow `systemctl --user`, slow USB enumeration, slow DNS each get bounded.

### 9.6 Probe results in the Logging window

The Logging window's Environment-probes section displays the most recent snapshot summary inline (status dot + one-line description per probe). `Re-run probes` triggers `logging_env_probes_rerun`. This gives operators self-service environmental diagnostics without exporting a log.

Updates flow via the push subscription described in §8.8.

### 9.7 Runtime CMS health state — supporting cache for the network probe (per Codex §4 Finding 5; placement amended per plan-adrev §5.1)

A new lightweight runtime state at `src-tauri/src/cms_health.rs` (**top-level crate module**, NOT under `winlink::session::*`):

```rust
// src-tauri/src/cms_health.rs
pub struct CmsHealthState {
    last_successful_contact_at: RwLock<Option<DateTime<Utc>>>,
    last_attempt_at: RwLock<Option<DateTime<Utc>>>,
    last_attempt_outcome: RwLock<Option<CmsAttemptOutcome>>,  // Success | TimeoutAfterMs | Refused | DnsFailed | ...
}
```

Updated by the actual CMS session code in `winlink::session::*` and `winlink::telnet*` on success/failure of connection attempts (those modules import `crate::cms_health::CmsHealthState` and call `record_success()` / `record_failure(...)`). The `network` probe reads from this state — and because the module sits at the crate root rather than under `winlink::session::*`, the probe's import is permitted by the §9.1 RADIO-1 isolation contract (which forbids `winlink::session::*` but does NOT forbid sibling top-level modules).

**Why top-level placement matters (plan-adrev §5.1):** the original spec v2 placed `cms_health` under `winlink::session::cms_health`. That created an apparent contradiction with §9.1's "probes never import `winlink::session::*`" rule. The fix is structural — move the module to the crate root so the probe's read-only state dependency does not violate the isolation invariant. The TX-touching code still lives in `winlink::session::*`; only the state record (which carries no TX behavior, only timestamps and an outcome enum) is hoisted out.

State is stored in Tauri-managed state so it persists across probe runs. This eliminates the "probe runs minutes after failure and infers from nothing" failure mode Codex flagged. The probe says "last CMS contact at T; X minutes ago" — accurate, not synthesized.

---

## 10. Acceptance criteria (expanded per Codex §11)

The PR is mergeable when ALL of the following hold.

### 10.1 Functional (each criterion maps to a concrete test, not a vague "operable" claim)

1. `tracing` macros (`info!`, `debug!`, `warn!`, `error!`) emit from every cluster in the §4.1 matrix. Verification: `tests/emission_coverage_test.rs` runs the full app subscriber, triggers synthetic operations exercising every cluster, asserts at least one event per cluster appears in the produced JSONL.
2. The Logging window opens from `Help → Logging…` and renders all three sections (Export, Settings, Environment probes). Verification: a Tauri integration test (`tests/logging_window_smoke.rs`) opens the window via `logging_window_open`, asserts the WebView is created with the expected label, asserts the route renders within 2s. UI control coverage per §10.1.1.
3. `Help → Report Issue` triggers auto-export. The flow handles each failure path:
   - Save As dialog success → archive produced + browser opened.
   - Save As dialog canceled → modal shows "canceled" message + no browser open.
   - Save As fails (permission denied) → modal shows error + Copy-template fallback.
   - Browser open fails (no default browser) → modal shows URL + Copy buttons.
4. Export produces `tuxlink-logs-{UTC-ts}-{attempt-id}.tar.zst` containing `summary.txt`, `events.jsonl.zst`, `dict.zdict`, `manifest.json` per §3.3–§3.5. Manifest's `compression.dict_amortized_ratio` ≥ 1.3 on the synthetic corpus benchmark. Tar members are mode 0600, uid=0, deterministic mtime.
5. Retention sweep deletes oldest closed files when either cap is hit, AND never deletes the active file. Test: `tests/retention_sweep_test.rs` populates the log dir with timestamped files (including the active file), runs sweep with various retention values, asserts active file is preserved and the right closed files are deleted in the right order.
6. Detailed mode auto-reverts after the operator's chosen window; revert event (`logging.detailed_mode.expired`) appears in the log. Test: invoke `logging_set_detailed_mode(Bounded(1.hour))`, advance time, assert revert.
7. Six environment probes emit at startup (after first paint) AND on first error from their respective subsystems. Probe outputs contain ONLY allowlisted env-var values. Test: assert each probe emits a `probe-snapshot` event after first paint within 12s; assert error-trigger debounce works (multiple errors → one probe run); assert the env-var allowlist excludes any name matching the exclusion regex.

### 10.1.1 UI control coverage (per Codex §11 Finding 2)

Each Logging-window control has a concrete expected-state test:
- **Export logs button** → Save As dialog opens; on success, archive at chosen path; on cancel, no error popup, modal closes. (3 cases)
- **Open log directory button** → `tauri-plugin-shell::open` invoked with the resolved log directory path. (1 case)
- **Clear history button** → confirmation modal appears; on confirm, all closed files deleted + ring buffer drained; on cancel, no-op. (2 cases)
- **Detailed mode Off radio** → `logging_set_detailed_mode(Off)`; UI state-text updates. (1 case)
- **Detailed mode On radio** → `logging_set_detailed_mode(On)`; status text shows "Until manually disabled." (1 case)
- **Detailed mode Bounded radio + hour input** → invalid hours (0, negative, >720, non-numeric) show inline validation error; valid hours invoke `logging_set_detailed_mode(Bounded(hours))`; status text shows countdown. (4 cases)
- **Retention days input** → invalid (0, >365, non-numeric) shows validation; valid invokes `logging_set_retention(days=N, mb_cap=...)`. (3 cases)
- **Retention MB/GB input + unit selector** → unit conversion works correctly; bounds 50 MB – 10 GB enforced; out-of-range shows validation. (3 cases)
- **Re-run probes button** → `logging_env_probes_rerun` invoked; updated snapshot pushed via `logging://probes/snapshot-updated`. (1 case)

### 10.2 Redaction-safety (correctness-critical, must have tests)

8. **Unit**: every name in §5.2 blocklist matches the regex. Each control case (`password_hint_index`, `challenge_round_number`, `nonce_count_total`, `key_event_handler`, `cookie_jar_path`) does NOT match.
9. **Unit**: every credential-bearing struct from §5.3 (source-verified list, primarily `ExchangeConfig`) has a `Debug` impl returning the `<redacted ...>` form. The `static_assertions` + `build.rs` source-scan asserts new structs with `password: String`-shape fields without manual `Debug` fail the build.
10. **Unit**: the `tracing::field::Visit` impl correctly handles ALL of `record_debug`, `record_str`, `record_error`, `record_bytes`, `record_i64`, `record_u64`, `record_i128`, `record_u128`, `record_bool`, `record_f64` (including non-finite floats per §3.1 encoding rules).
11. **Integration**: `tracing::debug!(password = %real_pw, ...)` (Display via `%`) → events.jsonl contains `"password":"<redacted>"`, NOT containing the real password string.
12. **Integration**: `tracing::debug!(?creds, ...)` where `creds: ExchangeConfig` carrying a password → events.jsonl contains `"<redacted>"` for the password field.
13. **Integration**: `tracing::debug!(byte_dump = &raw_bytes[..], ...)` where bytes contain known password material → 256-byte preview cap applied; password bytes NOT in the preview.
14. **CRITICAL integration**: full secure-login wire flow with known password `"hunter2hunter2"` and known challenge `"23753528"` → archive's events.jsonl does NOT contain the 8-digit token bytes computed from those values. Same for telnet password-response and peer P2P login flows.
15. **Lint/build**: opaque-container emission (`tracing::*!(payload = ?serde_json_value, ...)`) fails the build via the `no_opaque_container_emissions.rs` test.
16. **Smoke**: end-to-end "no secret bytes in archive" check in `tuxlink-logging-smoke.sh` — feeds known credentials through a synthetic auth flow, exports, greps archive for credentials, exits non-zero if found.

### 10.3 Decompression portability (agent-end ergonomics)

17. Archive decompresses with stock `tar` + `zstd ≥ 1.4.0`: `zstd -d archive.tar.zst && tar xf archive.tar && zstd -d -D dict.zdict events.jsonl.zst` produces valid JSONL. Outer archive does NOT require `--long` on decompression (per §7.1).
18. `summary.txt` is plain text, `grep`-readable, no escape sequences or binary content. ANSI escapes stripped, control characters absent.
19. Tar members extract with safe metadata: mode 0600, uid=0, deterministic mtime, no `..` paths, fixed member names.

### 10.4 Failure-mode tests (added per Codex §11 Finding 3)

20. **Active-writer export**: `tests/export_during_writes_test.rs` spawns a writer task emitting events at ~100/s, calls `logging_export` mid-stream, asserts the export completes without panic and the archive contains a valid JSONL (no truncated last line).
21. **Corrupt dictionary export**: replace `dict.zdict` asset with garbage bytes, build, run an export, assert dictionary validation fails, fallback to dict-free compression succeeds, manifest's `inner_dict_version` is `null`.
22. **D-Bus unreachable env probe**: simulate `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent`, run keyring probe, assert probe emits `dbus_reachable: false` and doesn't hang past the per-command deadline.
23. **ENOSPC during write**: simulate disk-full via a fixture filesystem (or `set -e; fallocate -l 100M /tmp/test.img; mkfs.ext4 /tmp/test.img; mount -o loop,size=100m /tmp/test.img /tmp/log_dir`), assert disk-write-error events emit and free-disk guard pauses cleanly.
24. **Symlink refusal**: create `$XDG_STATE_HOME/tuxlink/logs` as a symlink to `/tmp/elsewhere`, assert `state_dir::resolve()` refuses and emits the resolution-failed event.
25. **No-browser fallback**: simulate `tauri-plugin-shell::open` failure (no default browser), assert Report Issue modal shows the URL + Copy buttons.
26. **No-log export**: fresh install with zero rolled files + empty active file, click Export → archive is produced with `summary.txt` saying "0 events" and `events.jsonl.zst` being a 0-byte-zstd-empty.
27. **Clock-backward retention**: create files with timestamps both reflecting "older than retention_days" and with mtime "now", assert the disagree-by-more-than-1h grace prevents deletion + emits the disagreement warning.

### 10.5 Smoke artifacts

28. `scripts/tuxlink-logging-smoke.sh` exists, exits 0 on success, exercises: app starts → env probes emit (after first paint) → synthetic event sequence (including a secure-login flow with known credentials) → Export → unpack archive → verify summary.txt + events.jsonl content → grep for credential bytes → assert NOT FOUND. Agent-runnable, **EXPLICITLY zero RADIO-1 risk** (see §10.7).

### 10.6 Build pipeline

29. `cargo run --manifest-path xtask/Cargo.toml --target-dir xtask/target --bin gen-corpus -- --output dev/log-corpus-synthetic/` produces ~1.5–2 MB of synthetic events at `dev/log-corpus-synthetic/`, including the real-string fixtures listed in §7.3.
30. `cargo run --manifest-path xtask/Cargo.toml --target-dir xtask/target --bin train-log-dict -- --input dev/log-corpus-synthetic/ --output src-tauri/assets/logging/tuxlink-events-v1.zdict --size-kb 16` produces `src-tauri/assets/logging/tuxlink-events-v1.zdict` (~16 KB).
31. Both xtask binaries documented in new `xtask/README.md`.

### 10.7 RADIO-1 contract enforcement (per Codex §14 Finding 1)

32. **Compile-time isolation test** (`src-tauri/tests/probes_no_tx_apis.rs`): the probe modules do NOT import any of `winlink::session`, `winlink::secure`, `winlink::handshake`, `winlink::modem::ardop::command`, `winlink::modem::vara::commands`, `winlink::transfer`. Static-import check fails the build if a probe gains a TX-touching dependency.
33. **Runtime no-network-side-effects test** (`tests/probes_radio_safe.rs`): run all six probes against a mock environment with packet-capture wrapping the test process; assert NO outbound packets to CMS ports 8772/8773 beyond TCP SYN/RST (no payload). NO PTT GPIO writes. NO writes to any configured serial device.
34. **Smoke script RADIO-1 compliance**: `tuxlink-logging-smoke.sh` explicitly does NOT execute `native_cms_probe`, does NOT spawn VARA or ARDOP, does NOT open the real serial device. The script uses synthetic events from `gen-corpus` fixtures only. The script header documents this with a RADIO-1 compliance comment.

### 10.8 Adversarial review

35. At least one Codex adversarial round per `superpowers:build-robust-features` discipline. (This spec already had its spec-adrev round, captured in `dev/adversarial/2026-06-04-alpha-logging-spec-codex.md`; the implementation will require a build-phase round before merge.)

---

## 11. Operator smoke plan (<10 minutes)

Runnable once the PR merges and the converged build is up. **RADIO-1 compliance**: this smoke does NOT exercise live CMS auth, does NOT spawn VARA/ARDOP, does NOT open the configured radio serial port. All radio-touching diagnostic data comes from synthetic fixtures or read-only system queries.

```bash
# 1. Confirm app starts and env-probes fire after first paint.
pnpm tauri dev
# In another terminal:
tail -f ~/.local/state/tuxlink/logs/tuxlink.*.jsonl | grep env_probe
# Expect probe-snapshot events to appear within ~15s of the window painting.

# 2. Open Help → Logging. Verify window opens with Export / Settings / Environment-probes sections,
#    populated immediately from the cached startup snapshot.

# 3. Click "Export logs…" → save to /tmp/test.tar.zst.
#    Verify file exists; verify size is reasonable (KB to low-MB class for a fresh install).

# 4. Decompress the archive without tuxlink (proves recipient-side portability):
cd /tmp && mkdir x && cd x && zstd -d ../test.tar.zst && tar xf test.tar
cat summary.txt                                # human-readable
zstd -d -D dict.zdict events.jsonl.zst -o events.jsonl
head events.jsonl                              # valid JSONL, env_probe events visible
jq '.target' events.jsonl | sort -u            # see emitting clusters
jq '.attempt_id' events.jsonl | sort -u | head # see attempt-id distribution

# 5. In the Logging window, enable Detailed mode Bounded for 1 hour. Verify time-remaining displays.

# 6. Wait a few minutes; click Help → Report Issue.
#    Verify modal appears with file path; browser opens with pre-filled GitHub issue template;
#    drag-drop instructions visible. ABORT the GitHub flow (don't actually file).

# 7. Optional: shrink retention to 1 day / 50 MB in Logging window.
#    Verify a retention sweep event appears in the log within a few seconds,
#    and the active log file is NOT deleted.

# 8. Run the agent-runnable smoke script (RADIO-1 compliant — synthetic events only):
bash scripts/tuxlink-logging-smoke.sh
# expect: exit 0, "PASS" output including the "no secret bytes in archive" assertion result.

# 9. Operator-only manual check: in the exported archive, verify that:
#    - manifest.json's resolved log_dir is your actual $XDG_STATE_HOME path
#    - manifest.json's compression.dict_amortized_ratio is > 1.0 (ideally > 1.3)
#    - No CMS/VARA/ARDOP TX-shaped events appear in events.jsonl (since you haven't initiated any
#      transmissions during this smoke — RADIO-1 verification)
```

---

## 12. Out-of-scope deferrals

Explicitly NOT in the first-slice PR:

- **Real-corpus dictionary retraining** — v1 (single asset swap).
- **Per-subsystem verbosity sliders** — post-alpha; depends on operator demand surfacing.
- **In-app diagnostic log viewer** — separate UI work; the existing radio-panel session-log strip is not a general diagnostic live tail.
- **Allowlist-based redaction promotion** — beta or later; depends on surface area becoming too big for per-callsite review.
- **`gh` CLI detection / GitHub PAT integration** — post-alpha if URL-pre-fill friction surfaces.
- **Per-attempt level elevation** — earlier brainstorm proposal; explicitly rejected in favor of the simpler Off/On/Bounded operator-facing model.
- **Tiered exports (TINY / COMPACT / FULL picker)** — earlier brainstorm proposal; rejected as a foot-gun per operator direction.

These each become bd-issue follow-ups during `superpowers:writing-plans` decomposition.

---

## 13. Risk acknowledgment

- **Big-bang PR shape is heavy.** Mitigation: comprehensive acceptance criteria above, runnable smoke script, redaction unit + integration tests, env probes covered explicitly as alpha-gate requirements. Per Codex §6 Finding 3: active worktrees touching VARA / ARDOP / forms / packet / tuxmodem / docs / UI areas create real merge-conflict risk. Coordination plan: pause new branches in those areas during the PR's review-and-merge window OR rebase the logging PR onto main after each significant integration. Per operator framing this session: "ships as a working feature or it doesn't."
- **Synthetic-corpus-trained dictionary may compress real logs worse than ideal.** Mitigation: zstd dictionary mode falls back to dictionary-free compression on unmatched patterns — floor is "dict is wasted weight," not "dict actively hurts." v1 retraining from real corpus is a single asset swap. Compression-ratio telemetry in `manifest.json` (added per §7.5) makes the v1 retrain trigger decision data-driven.
- **Codex adversarial round is mandatory** (one already completed against this spec; build-phase round still required). Redaction logic + dictionary training corpus + env-probe allowlists each warrant independent scrutiny. Per memory `[[no-carveout-on-cross-provider-adrev]]` this is correctness-critical territory, not plumbing.
- **Existing `SessionLogState` ring buffer reuse** keeps the radio-panel session-log strip working without React-side change, but the new ui_consumer task must construct `LogLine` identically to the existing winlink_backend bridge or live-events will diverge in shape. Mitigation: a parity test asserts that the new consumer's output for a given tracing event equals the existing bridge's output for the equivalent legacy emission.
- **Wire-text leak surface remains the highest-risk path.** Even with the WireSanitizer in place (§5.6), a new wire-emitting callsite that forgets to route through `sanitize_wire_line()` would leak credentials. Mitigation: the §10.2 "no secret bytes in archive" end-to-end test catches this for the currently-known credential flows; the per-callsite-discipline requires code-review vigilance for future wire-emitting code.
- **Pre-first-paint events are not captured to disk.** Per §2.6, the temporary stderr subscriber covers the early startup window. Errors during config-parse or Tauri-builder construction don't reach the archive. Mitigation: those errors fall into the OS shell stderr capture (operator running `pnpm tauri dev` sees them in the terminal); a future enhancement could append a captured-stderr file to the archive.

---

## 14. Memory references

This design observes:
- `alpha-is-vettedness-not-built-ness` — first-slice ships fully built including env probes
- `no-tuxlink-added-safeguards` — applies to TX-path behavior; logging has no TX surface
- `inline-ui-no-window-clutter` — exception justified per §8.1 (infrequent admin, mirrors `help_window.rs`)
- `no-disk-creds-default` — drives the redaction layer's password-only scope
- `no-incomplete-or-internal-refs-in-shipped-features` — env probes shipped together with the export feature (rather than as a placeholder for a follow-up PR)
- `discipline-triage-rule` — Codex round IS warranted here (correctness-critical), unlike pure plumbing
- `no-carveout-on-cross-provider-adrev` — §10.8 mandates the build-phase Codex round (the spec-phase round has already been conducted; see §16)
- `no-stretched-full-width-ui` — Logging window constrained, no full-width stretch
- `explicit-referents-in-specs` — this spec names the feature + state at every reference

---

## 15. Implementation rollout (decomposition target for writing-plans) — REORDERED per Codex §6 Finding 2

The big-bang PR's logical groupings, ORDERED so that redaction + formatter + tests land BEFORE any credential-adjacent emission callsite. This sequencing is intentional: mid-PR partial progress (a reviewer reading commit N before commits N+1, N+2 land) must NOT produce a state where credential-adjacent callsites exist without their redaction defenses in place.

**Commit order within the single big-bang PR:**

1. **Infra foundation (commits 1-3)**
   - `logging/` module skeleton (`mod.rs`, `subscriber.rs`, `filter_layer.rs`, `settings.rs`)
   - `redact.rs` + `wire_sanitize.rs` + complete `tracing::field::Visit` implementation
   - Unit tests for redaction blocklist + wire sanitizer + Visit method coverage

2. **Credential-struct Debug impls (commit 4)**
   - Add manual `Debug` for `ExchangeConfig`
   - Audit credential-bearing struct list; add Debug impls or assert existing ones (per §5.3 source-verified list)
   - Add `tests/credential_debug_audit.rs` static-assertion test
   - Add `tests/logging_blocklist_corpus.rs` repo-derived field-name test

3. **Disk layer + retention (commits 5-7)**
   - `tracing-appender` integration, rolling files
   - `state_dir::resolve()` with fallback chain + symlink refusal + perm setting
   - `retention.rs` sweep logic (active-file protection, clock-backward grace)
   - `free_disk_guard.rs` with appender error observation
   - Unit + integration tests for sweep

4. **Export + compression (commits 8-10)**
   - `xtask` crate skeleton + `gen-corpus.rs` (with real-string fixtures) + `train-log-dict.rs`
   - `dict.rs` (validation + fallback) + asset bundling
   - `export.rs` (archive builder, tar normalization, flush barrier, summary/manifest rendering)
   - Tests: dict validation, dict-free fallback, corrupt-dict, no-log export, active-writer export, compression-ratio telemetry assertion

5. **Six env probes (commits 11-13)**
   - Probe trait + dispatch + RADIO-1 contract enforcement (no-tx-apis static test)
   - Six probe implementations: keyring, audio, serial, modem_process, network, display
   - `cms_health.rs` runtime state for the network probe
   - Per-probe unit tests + RADIO-1 runtime test

6. **Logging window (backend) (commit 14)**
   - `logging_window.rs`, Tauri commands per §8.4, settings persistence to TOML

7. **Logging window (frontend) (commits 15-16)**
   - `/logging` route + `LoggingView.tsx` + tests
   - `ReportIssueModal.tsx` + tests
   - `routing.ts` + `App.tsx` extension

8. **Report Issue flow (commit 17)**
   - `report_issue_flow` command, GitHub URL pre-fill with Markdown escaping + length check
   - `.github/ISSUE_TEMPLATE/bug.md`
   - Browser-open / Save-As failure-path tests

9. **Emission rollout (commit 18 — the largest commit, last to land)**
   - `tracing::*!` calls across every cluster in §4.1
   - Span instantiation per §4.5
   - Wire-emitting callsites updated to route through `sanitize_wire_line`
   - Message-body callsite policy enforced per §4.4.1

10. **Tests + smoke script (commit 19)**
    - `scripts/tuxlink-logging-smoke.sh` (RADIO-1 compliant per §10.7)
    - End-to-end "no secret bytes in archive" test
    - Failure-mode tests per §10.4

11. **Build-phase Codex adversarial round** — per §10.8. Conducted AFTER all commits land in the PR branch, BEFORE merging to integration.

The "stacked commits in one PR" pattern is what makes this manageable: reviewers can step commit-by-commit, knowing redaction defenses are in place before emissions land. The PR description includes a reviewer-checklist mapping commits → §10 acceptance criteria.

`superpowers:writing-plans` will translate this commit order into the bd-issue decomposition + per-step verification checkpoints.

---

## 16. Adversarial-review disposition

This spec underwent a Codex adversarial review in this session; the full transcript is at `dev/adversarial/2026-06-04-alpha-logging-spec-codex.md` (gitignored, local-only). 1 CRITICAL + ~16 HIGH + ~22 MEDIUM + ~2 LOW findings.

**Findings addressed in this v2 of the spec:**

| Codex section | Finding | Status |
|---|---|---|
| §1 Redaction completeness | CRITICAL: wire-text leak (`;PR:` plaintext) | §5.6 WireSanitizer added |
| §1 Finding 2 | Architectural: redaction-as-Layer cannot scrub immutable events | §2.2 architecture redesigned (Fanout Layer, single-format-once) |
| §1 Finding 3 | Blocklist regex misses credential-shaped names | §5.2 regex expanded with 15+ new patterns |
| §1 Finding 4 | Fabricated credential-struct audit list | §5.3 source-verified against `origin/main` |
| §1 Finding 5 | Incomplete `tracing::field::Visit` implementation | §5.7 added; all Visit methods specified |
| §1 Finding 6 | Nested-container redaction bypass | §5.7 lint + recursive-redact policy added |
| §2 Finding 1 | Singular `span` field cannot hold span stack | §3.1 changed to `spans` array + top-level `attempt_id` |
| §2 Finding 2 | Missing optional v1 fields (pid, thread, source) | §3.1 added pid, thread, module, file, line |
| §2 Finding 3 | JSON encoding gotchas (NaN, control chars, length) | §3.1 JSON encoding rules added |
| §2 Finding 4 | Span absent vs null distinction undefined | §3.1 evolution rules clarify |
| §3 Finding 1 | `--long=27` may break older zstd | §7.1 dropped from outer archive |
| §3 Finding 2 | Tar member metadata unnormalized | §7.6 tar normalization rules added |
| §3 Finding 3 | Dict validation in wrong layer | §7.5 moved to export init |
| §4 Finding 1 | Probes miss real backend variants | §9.3 expanded coverage |
| §4 Finding 2 | Env allowlist gaps | §9.4 expanded allowlist + exclusion regex |
| §4 Finding 3 | Probe-on-error storm risk | §9.2 debounce + single-flight pattern |
| §4 Finding 4 | Sync probes block startup | §9.5 deferred to after first paint |
| §4 Finding 5 | Network probe synthesizes CMS-contact data | §9.7 `cms_health.rs` runtime state added |
| §5 Finding 1 | Synthetic corpus too clean | §7.3 expanded with real-string fixtures |
| §5 Finding 2 | No compression-ratio acceptance | §7.5 manifest telemetry + acceptance criterion |
| §6 Finding 1 | Touched-files list underestimates | §2.4 full inventory added |
| §6 Finding 2 | Implementation groupings not safely orderable | §15 reordered: redaction-and-tests before emissions |
| §6 Finding 3 | Merge-conflict risk with concurrent VARA/ARDOP work | §13 risk + coordination plan added |
| §7 Finding 1 | Matrix omits orchestration modules | §4.1 orchestration cluster added |
| §7 Finding 2 | Message-body callsite policy undefined | §4.4.1 added (never log full body) |
| §7 Finding 3 | Cross-process surfaces (helper bins, tuxmodem) unscoped | §2.4 + §12 explicit OUT OF SCOPE |
| §8 Finding 1 | Seq race between UI/disk allocators | §2.2 single Fanout allocator + §2.5 append_with_seq |
| §8 Finding 2 | Retention sweep can delete active file | §6.3 active-file protection rule |
| §8 Finding 3 | Export-during-write race | §6.5 flush barrier + partial-line tolerance |
| §8 Finding 4 | Reload of filter state non-atomic | §6.5 `tracing_subscriber::reload` for atomic swap |
| §9 Finding 1 | Init owner / guard lifecycle undefined | §2.6 single init owner + LoggingHandle pattern |
| §9 Finding 2 | Routing files missing from touched-files | §2.4 updated |
| §9 Finding 3 | Save As / browser-open failure paths underspecified | §8.5 each path handled |
| §9 Finding 4 | Push vs poll for probe UI updates | §8.8 Tauri event subscription |
| §10 Finding 1 | ENOSPC/EIO mid-poll handling | §6.4 appender error observation |
| §10 Finding 2 | Sudo/Flatpak/permission-denied | §6.1 resolution order + fail-soft |
| §10 Finding 3 | Clock-backward filename-sort | §6.3 mtime + filename agreement rule |
| §11 Finding 1 | Redaction acceptance too narrow | §10.2 expanded (12 → 16 criteria) |
| §11 Finding 2 | "All controls operable" vague | §10.1.1 per-control test mapping |
| §11 Finding 3 | Smoke can pass while real flows fail | §10.4 failure-mode test catalog |
| §12 Finding 1 | Missing research doc reference | Acknowledged; doc lives on `origin/main` at `dev/research/2026-06-04-winlink-group-pain-points.md` (commits 6643f96 + 8c5b098); current branch (`bd-tuxlink-xygm/recover-handoffs`) lacks it but the reference resolves at merge |
| §12 Finding 2 | Filename convention inconsistency | §6.2 standardized + glob fixed |
| §12 Finding 3 | Double init in lib.rs and main.rs | §2.6 single owner clarified |
| §12 Finding 4 | "No machine-specific paths" claim vs path emissions | §1 clarified — paths ARE allowed in event fields; the "portable" property is about archive layout + tooling, not absence of paths |
| §13 Finding 1 | Symlink + permissions unspecified | §6.2 added 0700/0600 + symlink refusal |
| §13 Finding 2 | Tar extraction hostile-archive safety | §7.6 fixed safe member names; acknowledged for archive recipients |
| §13 Finding 3 | Export permissions on shared machines | §6.2 archives written 0600 |
| §13 Finding 4 | GitHub URL Markdown injection | §8.5 escape rules + length cap |
| §14 Finding 1 | Probes not RADIO-1 constrained | §9.1 RADIO-1 contract added + §10.7 enforcement tests |
| §14 Finding 2 | TX-path logging perturbs timing | §13 risk acknowledged; structured-field emission discipline reduces hot-path allocation |
| §14 Finding 3 | Smoke RADIO-1 compliance unspecified | §11 + §10.7 explicit RADIO-1 prohibitions |

**Findings NOT adopted (with reasoning):**

None outright rejected. A few findings had minor scope adjustments (e.g., Codex's "expand emission cluster" suggestions were partially adopted — added the orchestration cluster but kept `wizard`/`bootstrap`/`config` at info-default since they're one-shot lifecycle events without ongoing emission demand). Where Codex's exact recommendation would have over-scoped first-slice work (e.g., spawning a separate tuxmodem-logging subsystem), the spec defers via §12 with an explicit out-of-scope rationale.

---

## 17. Plan-adrev disposition (spec amendments)

A separate Codex round reviewed the implementation plan that derives from this spec (transcript at `dev/adversarial/2026-06-04-alpha-logging-plan-codex-v2.md`, gitignored). Most findings landed in the plan v2. Two findings required spec amendments captured here:

### 17.1 `cms_health` module placement (plan-adrev §5 Finding "Probe isolation conflicts with cms_health placement")

**Original spec v2:** §9.7 placed `CmsHealthState` at `src-tauri/src/winlink/session/cms_health.rs`.

**Problem:** §9.1's RADIO-1 isolation contract forbids probe modules from importing `crate::winlink::session::*`. §9.2 requires the network probe to read `CmsHealthState`. These are mutually exclusive when the state lives under the forbidden path.

**Spec v2.1 amendment:** module placement moved to top-level crate root at `src-tauri/src/cms_health.rs`. Probe imports `crate::cms_health::CmsHealthState` (permitted). TX-touching session code remains under `winlink::session::*` (unchanged); it imports the state from its new top-level location. §9.7 prose updated to reflect this; the isolation test in §10.7 #32 remains as written (the forbidden-imports list still names `crate::winlink::session::`).

### 17.2 Dictionary-validation mechanism clarification (plan-adrev §5 Finding "Dictionary validation approach needs spec clarification")

**Original spec v2:** §7.5 stated "validation runs once at process start ... `zstd::dict::DecoderDictionary::copy(bytes)`".

**Problem:** `DecoderDictionary::copy` does not return a `Result` — it cannot signal "the bytes are not a valid zstd dictionary." The acceptance criterion #21 ("corrupt dictionary export ... assert dictionary validation fails, fallback to dict-free compression succeeds") cannot be satisfied with the stated mechanism.

**Spec v2.1 amendment:** §7.5 is amended to specify a concrete validation: at export-pipeline initialization, the bundled dictionary is exercised via a known-input compress + decompress roundtrip (e.g., `b"tuxlink-dict-check-2026"`). If either step errors, the dictionary is marked invalid; export falls back to dictionary-free compression with `inner_dict_version: null` in manifest; a `warn`-level `dict-invalid: falling back to dict-free compression` event records the fallback. Acceptance criterion #21 maps to this concrete mechanism.

### 17.3 What's NOT amended

Other plan-adrev findings (CRITICAL FanoutLayer-impl-shape, HIGH free-disk-pause-flag wiring, HIGH retention-sweep-rotation-trigger, etc.) are plan-shape issues addressed inline in the implementation plan v2, not spec-shape issues. The spec's architectural decisions stand; the plan's *encoding* of those decisions needed fixes.

---

**End of spec.**

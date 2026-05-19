# Main-UI Cluster — Technical Design Spec (v0.0.1 Tasks 8 / 12 / 13 / 14 / 15 / 16)

> **Status:** Authoritative implementation contract for the v0.0.1 main-UI cluster. Consolidates the canonical UX (`docs/design/v0.0.1-ux-mockups.md` §5.5–§5.9 + §3–§4), the v0.0.1 plan's Task 8/12–16 implementation guidance (`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`, AMD-6..10 amended), and the **shipped** backend surface (`src-tauri/src/winlink_backend.rs` + `pat_client.rs`). Authored 2026-05-19 by `oriole-lichen-bayou` under `dev/plans/2026-05-19-autonomous-ui-buildout.md` Phase 0. One Codex cross-provider adrev round applied (see disposition table at end).
>
> **This spec supersedes the v0.0.1 plan's per-task code snippets where they conflict.** Those snippets were authored 2026-04-22, before the `WinlinkBackend` trait (`tuxlink-z5f`), the async `PatClient` refactor, and the cred-handling refactor (`tuxlink-mib`) shipped. The snippets are stale on five concrete points (§1.2). The plan's *file paths, task structure, and acceptance criteria* remain guidance; the *API surface* is defined here against shipped code.

---

## 1. Architecture thesis

### 1.1 All six tasks route through the `WinlinkBackend` trait held in Tauri managed state

The shipped architecture (`winlink_backend.rs`) defines `trait WinlinkBackend: Send + Sync` with `PatBackend` as the v0.0.1 implementation. The main-UI message operations **consume this trait**; they do NOT construct raw `PatClient` instances per-command, and they do NOT invent a parallel API.

The app holds a single backend instance in Tauri managed state:

```rust
// src-tauri/src/app_backend.rs  (NEW — owned by Task 12)
// RwLock (not Mutex): set once at bootstrap, read by every command. The trait
// is Send+Sync, so commands clone the Arc, DROP the lock, THEN await — never
// hold the lock across an .await (trait invariant, winlink_backend.rs:223).
pub struct AppBackend(pub std::sync::RwLock<Option<std::sync::Arc<dyn WinlinkBackend>>>);
```

- **Registered in `lib.rs`** (NOT `main.rs`): this repo's `tauri::Builder` — `.manage(...)`, `.setup(...)`, `.invoke_handler(...)`, `.on_window_event(...)` — all live inside `tuxlink_lib::run()` in `lib.rs` (verified `lib.rs:29`); `main.rs` is a 4-line `tuxlink_lib::run()` shim. **Every "register a command / managed state / setup hook" instruction in this spec means `lib.rs`.** The v0.0.1 plan snippets that show a `fn main()` `Builder` are stale on this point too.
- Command access pattern (mandatory — no lock held across await):
  ```rust
  let backend = { state.0.read().unwrap().clone() }; // clone Arc, drop guard
  let backend = backend.ok_or(UiError::NotConfigured("backend offline".into()))?;
  let metas = backend.list_messages(folder).await?;   // await with no lock held
  ```
- Populated at app start: if tuxlink config exists AND `connect_to_cms == true`, the bootstrap spawns Pat, constructs a `PatBackend`, stores `Arc<PatBackend>`. Offline-mode installs leave it `None`.
- `None` → `UiError::NotConfigured` projected to the UI as a "not connected" empty state (NOT an error toast).

**Rationale:** the trait is the architectural boundary (`tuxlink-z5f`); per the playbook, "define the IPC contract against what already exists, not invent a parallel one." A single managed instance gives every command the same status cache + log broadcast, which Tasks 15 (log stream) and 16 (status) need.

### 1.2 The five stale snippets this spec corrects

| Plan snippet (2026-04-22) | Shipped reality (2026-05-18) | Spec resolution |
|---|---|---|
| `PatClient::list(folder)` synchronous | `PatClient`/`WinlinkBackend` methods are **async** (`.await`); `reqwest::blocking` panics inside Tauri's runtime | All backend Tauri commands are `async fn`, `.await` the trait |
| `session.rs` `OnceLock<String>` URL + `PatClient::new(url)` per command | `PatBackend` wraps client + status + log broadcast | `AppBackend` managed state holds `Arc<dyn WinlinkBackend>` (§1.1) |
| `message_read` returns parsed `FullMessage{subject,from,to,body}` | `read_message` returns `MessageBody{ raw_rfc5322: Vec<u8> }` ("parse at display boundary") | Task 13 parses RFC5322 at the command boundary (§5.3); needs a parser dep (§8) |
| Task 14 `Compose` uses `@radix-ui/react-dialog` | AMD-6: **separate Tauri window** | §5.4 specifies `WebviewWindowBuilder`; Radix Dialog snippet is void |
| Task 15 raw-`String` `LogRing` + `session_log_snapshot` polling | trait exposes `stream_log() -> BoxStream<'static, LogLine>` (structured) | §5.5 routes Task 15 through `stream_log` → Tauri event channel; Human/Raw projection over structured `LogLine` |

---

## 2. Message data model

### 2.1 Trait additions (owned by Task 12 — the model root)

The shipped `MessageMeta` is missing fields the list view requires. Task 12 extends the trait surface (`winlink_backend.rs` + `pat_client.rs` DTO + `From` impl + `PatBackend::list_messages` mapping):

```rust
// winlink_backend.rs — MessageMeta gains two fields (additive; #[non_exhaustive] already)
pub struct MessageMeta {
    pub id: MessageId,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,        // ADDED — the list "To" column (esp. Sent). Pat DTO has a To array.
    pub date: String,           // RFC 3339 UTC
    pub unread: bool,
    pub body_size: u64,
    pub has_attachments: bool,  // ADDED — the "#" attachment-indicator column
}
```

`pat_client.rs`: `PatMessageDto` gains `to` (`#[serde(rename = "To", default, deserialize_with = deser_addr_list)]`) and a `has_attachments` field. **⚠ Codex VERIFIED the *absence* of these fields in our types, but NOT that Pat 1.0.0 *provides* them** — the spec's earlier "Pat DTO has a To array" assumption is INFERRED, not source-backed. The implementer MUST confirm against a real Pat `/api/mailbox` JSON fixture during TDD. **Graceful degradation is mandatory:** if Pat's list DTO omits `To` and/or attachment metadata, `to` defaults `[]` (the "To" column shows the folder-appropriate fallback — sender for Inbox, blank for Sent) and `has_attachments` defaults `false` (the `#` column is suppressed). Report as DONE_WITH_CONCERNS so the orchestrator + M2 smoke confirm the real Pat shape.

**`read_message` folder generalization (owned by Task 12):** `PatBackend::read_message` currently hardcodes `MailboxFolder::Inbox`. Reading a Sent message requires the folder. The trait method signature is kept (`read_message(&self, id: &MessageId)`); instead the **read path takes the folder via a `ReadOptions`-free overload is over-engineering for v0.0.1** — simplest: add `read_message_in(&self, folder: MailboxFolder, id: &MessageId)` to the trait (default-method-free; both `PatBackend` and `NativeBackend` implement it; `read_message` becomes `read_message_in(Inbox, id)`). Task 12 owns this trait change so Task 13 builds on a complete surface.

### 2.2 Folder model

Sidebar folders map to the trait's `MailboxFolder { Inbox, Sent, Outbox, Archive }` as follows:

| Sidebar folder | v0.0.1 backing | Notes |
|---|---|---|
| **Inbox** | `MailboxFolder::Inbox` | functional |
| **Outbox** | `MailboxFolder::Outbox` | functional (queued, not-yet-sent) |
| **Sent** | `MailboxFolder::Sent` | functional |
| **Drafts** | local (`localStorage`, Task 14's draft store) | NOT a backend folder; sidebar reads `listDraftIds()` |
| **Deleted** | **disabled placeholder** (v0.0.1) | trait has no `delete`/`move`; real delete needs a trait method (deferred to v0.1, named in §9) |
| **Templates** | disabled placeholder | per design doc §5.5 |

### 2.3 TypeScript model (owned by Task 12 — `src/mailbox/types.ts`)

```ts
export interface MessageMeta {
  id: string;            // MID
  subject: string;
  from: string;
  to: string[];
  date: string;          // RFC 3339 UTC
  unread: boolean;
  bodySize: number;
  hasAttachments: boolean;
}

// Reading-pane parsed view (Task 13 produces this from raw_rfc5322 at the Rust boundary)
export interface ParsedMessage {
  id: string;
  subject: string;
  from: string;
  to: string[];
  cc: string[];
  date: string;          // RFC 3339 UTC
  body: string;          // decoded text/plain
  attachments: AttachmentMeta[];   // names + sizes; bytes fetched lazily (v0.1) — v0.0.1 lists names only
  isForm: boolean;       // body is a Winlink form payload → v0.1 placeholder
  routing: string | null;// e.g. "via CMS-SSL" — from header strip (§5.3); null if unknown
}

export interface AttachmentMeta { filename: string; size: number; }

export type MailboxFolder = 'inbox' | 'outbox' | 'sent' | 'drafts' | 'deleted';
```

---

## 3. IPC command contract

All commands live in a new `src-tauri/src/ui_commands.rs` module (owned by Task 12) and are registered in `lib.rs`'s `invoke_handler` (inside `tuxlink_lib::run()`; `main.rs` is just the shim — see §1.1). Each is `async`, takes `State<'_, AppBackend>`, and returns `Result<T, UiError>`.

### 3.1 Error projection (owned by Task 12)

`BackendError` carries non-serializable `#[source] Box<dyn Error>` fields. The UI gets a serializable projection mirroring the wizard's `#[serde(tag="kind", content="detail")]` shape (`src/wizard/types.ts` is the pattern):

```rust
// ui_commands.rs
#[derive(serde::Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum UiError {
    NotConfigured(String),
    NotFound(String),           // MID
    AuthFailed { reason: String },
    Transport { reason: String },
    Unavailable { reason: String },
    Rejected(String),
    Cancelled,
    Internal { detail: String },
}
```

**The `From<BackendError>` impl MUST be exhaustive** (Codex finding 6 — `BackendError` has `InvalidSession`, `Cancelled`, `NotImplemented`, `Io` that an earlier draft dropped). Mapping:

| `BackendError` | → `UiError` |
|---|---|
| `NotConfigured(s)` | `NotConfigured(s)` |
| `NotFound(id)` | `NotFound(id.0)` |
| `AuthFailed{reason}` | `AuthFailed{reason}` |
| `TransportFailed{reason,..}` | `Transport{reason}` (stringify `source`) |
| `BackendUnavailable{reason,..}` | `Unavailable{reason}` |
| `MessageRejected(s)` | `Rejected(s)` |
| `Cancelled` | `Cancelled` |
| `NotImplemented` | `Unavailable{reason:"not implemented in v0.0.1"}` |
| `InvalidSession` | `Internal{detail:"invalid session"}` |
| `Io(e)` | `Internal{detail: e.to_string()}` |
| `Internal{msg,..}` | `Internal{detail: msg}` (stringify `source` chain) |

`config_read` is NOT a backend call — its failures (config read/parse) map directly to `UiError::Internal`.

```ts
// src/mailbox/types.ts — mirrors UiError
export type UiError =
  | { kind: 'NotConfigured'; detail: string }
  | { kind: 'NotFound'; detail: string }
  | { kind: 'AuthFailed'; detail: { reason: string } }
  | { kind: 'Transport'; detail: { reason: string } }
  | { kind: 'Unavailable'; detail: { reason: string } }
  | { kind: 'Rejected'; detail: string }
  | { kind: 'Internal'; detail: { detail: string } };
```

### 3.2 Commands

| Command | Signature (Rust) | Consumed by | Trait call |
|---|---|---|---|
| `mailbox_list` | `async fn(folder: String, state) -> Result<Vec<MessageMetaDto>, UiError>` | Task 12 | `list_messages(folder)` |
| `message_read` | `async fn(folder: String, id: String, state) -> Result<ParsedMessageDto, UiError>` | Task 13 | `read_message_in(folder, id)` + RFC5322 parse |
| `message_send` | `async fn(draft: OutboundDraftDto, state) -> Result<Option<String>, UiError>` | Task 14 | `send_message(OutboundMessage)` |
| `backend_status` | `async fn(state) -> Result<StatusDto, UiError>` | Task 16 (optional) | `status()` |
| `config_read` | `async fn() -> Result<ConfigViewDto, UiError>` | Task 16 | reads `config.rs` (no backend) |

- `OutboundDraftDto { to: Vec<String>, cc: Vec<String>, subject: String, body: String }` → mapped to `OutboundMessage` with `date` set to `now()` RFC3339 at the command (the trait requires `date`; the UI does not supply it). **⚠ `cc` caveat (Codex finding 5, VERIFIED):** `PatBackend::send_message` + `PatClient::send` currently map only `to`/`subject`/`body`/`date` — **`cc` is silently dropped.** Task 14 must NOT present a Cc field that silently loses data: either (a) wire `cc` through `PatClient::send` (add the form field, verify Pat 1.0.0's `/api/mailbox/out` accepts it), or (b) disable the Cc field with a "Cc arrives in v0.1" tooltip. See §5.4.
- `message_send` returns `Option<String>` (MID) faithfully — **Pat 1.0.0 returns `None`**; compose UX must not depend on echoing back a server MID (§5.4).
- `folder: String` is parsed to `MailboxFolder` in Rust; `"drafts"`/`"deleted"` never reach a backend command (handled frontend-side / disabled).

### 3.3 Event channels (Task 15)

Session-log lines stream from `WinlinkBackend::stream_log()`. The bootstrap (Task 12) spawns a task that drains the `BoxStream<LogLine>` and emits a Tauri event per line:

```
event name: "session_log:line"
payload:     LogLineDto { timestampIso: string, level: 'trace'|'debug'|'info'|'warn'|'error', source: 'backend'|'pat'|'transport'|'wire', message: string }
```

A `session_log_snapshot` command returns the current ring buffer (last N lines) for late subscribers / pane re-open. Task 15's frontend listens to the event + seeds from the snapshot.

---

## 4. App-shell layout

### 4.1 Component tree + grid regions (canonical per `synthesis-dock-off.png`)

```
AppShell  (CSS grid; owned by Task 12)
├─ region "ribbon"     → DashboardRibbon      (Task 16)   [top, ~40px]
├─ region "sidebar"    → FolderSidebar        (Task 12)   [left]
├─ region "list"       → MessageList          (Task 12)   [center]
├─ region "reader"     → MessageView          (Task 13)   [right]
├─ region "dock"       → (Task 16.5 — OUT OF SCOPE; grid reserves the column, renders nothing)
├─ region "sessionlog" → SessionLog           (Task 15)   [bottom strip, ~120px resizable]
└─ region "statusbar"  → StatusBar            (Task 16)   [bottom, ~24px]
Compose                 → separate Tauri window (Task 14) — NOT in the shell grid
```

### 4.2 Selection-state ownership (the no-full-view-swap invariant)

`AppShell` (Task 12) owns two pieces of selection state and passes them down:

- `selectedFolder: MailboxFolder` — set by `FolderSidebar`; drives which folder `MessageList` queries.
- `selectedMessage: { folder: MailboxFolder; id: string } | null` — set by `MessageList`; drives what `MessageView` renders. **The folder is carried with the id** (Codex finding 1): `message_read` and `read_message_in` both require the folder, so a bare `selectedMessageId` would recreate the Inbox-only bug. `useMessage`'s query key is `[folder, id]`. Selecting a different folder resets `selectedMessage` to `null`.

**Invariant (design doc anti-pattern):** selecting a message updates ONLY the reader pane; selecting a folder updates ONLY the list (and resets `selectedMessage` to `null`). The whole view NEVER swaps. No router-driven full-page navigation for message/folder selection. (The compose *window* is the one permitted multi-window exception, per locked decision #2.)

### 4.3 Composition + merge-order contract

Per Codex verdict 5 — per-region rebasing is workable but conflict-prone on `AppShell.tsx` + `lib.rs`'s `invoke_handler`. **Adopt a single dedicated integration commit instead:**

1. **Task 12** creates `AppShell.tsx` + `App.tsx` wizard→shell routing. AppShell renders its own regions (sidebar/list) + reader as a "Select a message" placeholder, and **inline placeholder `<div>`s** for `ribbon`/`sessionlog`/`statusbar`. It registers ONLY its own commands in `lib.rs`'s `invoke_handler`.
2. **Tasks 8/13/14/15/16** build their components/commands in their **own files** with **standalone unit tests**. They add their `#[tauri::command]` *function* to `ui_commands.rs` (append-only, low conflict) but do **NOT** edit `AppShell.tsx` or `lib.rs`'s `invoke_handler`. (Task 8 is the exception — it owns `tray.rs` + its own `lib.rs`/`main.rs` tray wiring, no shared shell.)
3. **The orchestrator owns one INTEGRATION COMMIT** (after the component PRs merge, before M2): it edits `AppShell.tsx` (swap placeholders → real imports for ribbon/reader/sessionlog/statusbar), `App.tsx` (main-window-only `menu:file:new` → open compose; see §5.4), and `lib.rs` (`invoke_handler![...]` — register `message_read`, `message_send`, `backend_status`, `config_read`). This concentrates all shared-file edits into ONE diff, eliminating cross-PR conflicts on those hotspots.

This makes the message-model dependency (13/14 import Task 12's `types.ts`) the only *hard* build dependency; every other coupling collapses into the single integration commit. The components are each independently unit-tested before integration; M2 smokes the wired whole.

---

## 5. Per-task specifications

### 5.1 Task 8 — System tray + window-close-to-tray (`tuxlink-rit`) [independent; Wave 2A]

Rust-only; no frontend coupling. Per v0.0.1 plan Task 8 (the implementation is current — Tauri 2 `TrayIconBuilder` + `MenuItemBuilder`, and the Quit pattern matches the PR #71 fix: custom item + `on_menu_event` → `app.exit(0)`, NOT `PredefinedMenuItem::quit` which is Linux-unsupported).

- **Files:** create `src-tauri/src/tray.rs` + `src-tauri/tests/tray_test.rs`; modify `lib.rs` (`pub mod tray;`), `lib.rs` (inside `tuxlink_lib::run()`'s `Builder` — NOT `main.rs`: `tray::install` in `.setup()` + `on_window_event` CloseRequested → `window.hide()` + `api.prevent_close()`), `tauri.conf.json` (trayIcon), add `src-tauri/icons/tray-icon.png` (monochrome "T", 32×32).
- **Behavior:** tray menu = Show/Hide Window · New Message · Quit. Window close button hides to tray (process + Pat child stay alive — load-bearing for emcomm; closing mid-ARQ must not kill Pat). Only File→Quit / tray→Quit / Ctrl+Q exit. "New Message" emits `menu` event `menu:file:new` (Task 14 listens) after showing the window.
- **State:** none (Rust event handlers).

### 5.2 Task 12 — Folder sidebar + message list + model + backend bootstrap (`tuxlink-zsm`) [ROOT; Wave 2B]

The root. Owns the message model, the IPC foundation, the AppShell, and the backend bootstrap. **Tasks 13/14 are blocked on this merging** (they import `src/mailbox/types.ts`).

- **Files (create):** `src/mailbox/types.ts`, `src/mailbox/FolderSidebar.tsx`, `src/mailbox/MessageList.tsx`, `src/mailbox/useMailbox.ts`, `src/shell/AppShell.tsx`, `src-tauri/src/app_backend.rs`, `src-tauri/src/ui_commands.rs`, tests (`*.test.tsx`, `src-tauri/tests/ui_commands_test.rs`).
- **Files (modify):** `winlink_backend.rs` + `pat_client.rs` (MessageMeta `to`+`has_attachments`, `read_message_in`), `lib.rs` (managed state `.manage()`, `mailbox_list`/`config_read`-stub registration in `invoke_handler`, `.setup()` app-start bootstrap — all inside `tuxlink_lib::run()`; `main.rs` is only the shim), `App.tsx` (wizard→shell routing).
- **Behavior:** post-wizard, AppShell renders. FolderSidebar lists Inbox/Outbox/Sent/Drafts (counts) + Deleted/Templates disabled. Selecting a folder loads its messages into MessageList (virtualized via `react-virtuoso`). Columns: UTC time · From · To · Subject · `#` (attachment) · size (when nonzero). Unread rows bold. Empty state: "No messages yet. Press F5 or Session → Connect to check for new mail." `useMailbox(folder)` uses TanStack Query (`refetchInterval: 10_000`). Drafts folder reads `listDraftIds()` (local). NO full-view-swap (§4.2).
- **State owned:** `selectedFolder`, `selectedMessage` (in AppShell). Query cache (TanStack).
- **Trait/IPC:** `mailbox_list` → `list_messages`. Bootstrap: spawn Pat + construct `PatBackend` + store in `AppBackend` + drain `stream_log` → `session_log:line` events.

### 5.3 Task 13 — Message reading pane (`tuxlink-y5c`) [depends on 12; Wave 2C]

- **Files (create):** `src/mailbox/MessageView.tsx`, `src/mailbox/useMessage.ts`, tests. **Modify:** `ui_commands.rs` (append `message_read` command fn — registration in `invoke_handler` happens in the orchestrator integration commit, §4.3; do NOT edit `AppShell.tsx` or `lib.rs`'s `invoke_handler`).
- **Behavior:** renders the selected message. Header strip: sender callsign (+ grid in parens if known) · UTC sent · UTC received · routing ("via CMS-SSL"/"via Telnet" — from `BackendStatus.transport` or message metadata; `null` → omit). Body: decoded `text/plain` in a `pre` (wrap). Winlink form payload (`isForm`) → "This message contains a Winlink form. Form rendering arrives in v0.1." Attachments: a strip below the body listing names + sizes (v0.0.1 lists only; in-app preview + OS-open are v0.1 — NEVER spawn a browser). Empty (no selection): "Select a message to read."
- **RFC5322 parsing (the key new work):** `message_read(folder, id)` calls `read_message_in(folder, id)` → `Vec<u8>`, then parses headers + text/plain body + attachment names into `ParsedMessageDto`. Uses a parser dep (§8). Byte→UTF-8 lossy-decode at this boundary only. **Per Codex verdict 3: cap the parse input (e.g. reject/truncate > a few MB) and surface a parser-failure UI state** — if parsing fails, `MessageView` shows "This message could not be parsed (raw size N bytes)." rather than crashing or rendering garbage. `ParsedMessage` gains an implicit failure path via the command's `Result` (a parse error → `UiError::Internal`, rendered as the failure state).
- **State:** none beyond the `useMessage(folder, id)` query (key `[folder, id]`; `enabled: !!selectedMessage`). The folder comes from `selectedMessage.folder` (§4.2) — never assume Inbox.

### 5.4 Task 14 — Compose window (separate Tauri window) (`tuxlink-dm8`) [depends on 12; Wave 2C]

Per AMD-6 + locked decision #2 — **separate Tauri window, NOT Radix Dialog.**

- **Files (create):** `src/compose/Compose.tsx` (mounted at `/compose/:draftId` inside a window labeled `compose-<draftId>`), `src/compose/useDraft.ts` (localStorage draft store — this is also the "Drafts" folder source for Task 12), `src/compose/draft.test.ts`, `src-tauri/src/compose_window.rs` (`WebviewWindowBuilder`; per-window geometry via `tauri-plugin-window-state`). **Modify:** `ui_commands.rs` (append the `message_send` command fn — Task 14 owns send semantics), `lib.rs` (`compose_window_open` command + register `tauri-plugin-window-state` inside `tuxlink_lib::run()` — NOT `main.rs`). **Does NOT edit `App.tsx`** — the `menu:file:new` → open-compose wiring lands in the orchestrator integration commit (§4.3), gated to the MAIN window only (Codex finding 7: menu events broadcast to every webview via `menu.rs`'s `app.emit`, so a compose window must NOT listen for `menu:file:new` or it spawns nested compose windows).
- **Behavior:** New Message opens a separate window; multiple allowed; survives main-window-hide-to-tray. Title "New Message — Tuxlink" / "Re: <subject>". Fields per design §5.7: **From** (disabled, single callsign, v0.1+ tooltip), **Send as** (disabled, "Winlink Message", v0.1+ tooltip), **To** (semicolon-separated), **Cc** (semicolon-separated — but per the `cc` caveat in §3.2, Pat currently drops it: either wire `cc` through `PatClient::send` after verifying Pat accepts it, or DISABLE the Cc field with a v0.1 tooltip; never silently drop), **Subject**, **Body**, **Select Template** (disabled, v0.1+ tooltip), **Attachments** button + drop zone (adds to a list — never a sub-window; v0.0.1 may stub attach-send as DONE_WITH_CONCERNS if Pat multipart attachment wiring is out of reach), **Request ack receipt** checkbox, **Post to Outbox** button, **Save Draft**. Autosave to `localStorage` every 2s; restored on reopen; cleared on successful send. Closing with unsaved changes → "Save draft / Discard / Cancel" (never silently discard). Ctrl+S save, Ctrl+Enter send. `message_send` returns `Option<MID>` — show "Posted to Outbox" on `Ok(_)` regardless of `Some`/`None` (Pat returns `None`).
- **State:** `draft` (local component + localStorage); window geometry (plugin).

### 5.5 Task 15 — Session log pane (`tuxlink-69z`) [independent build; Wave 2A]

Per AMD-7 + locked decision #3 — Human/Raw projections of ONE structured stream.

- **Files (create):** `src/session/SessionLog.tsx`, `src/session/logProjection.ts` (+ tests). **Modify (wiring, rebased on 12):** `AppShell.tsx` (sessionlog region). The backend event emission + `session_log_snapshot` command are part of Task 12's bootstrap (§3.3) — Task 15 consumes them; its frontend is unit-tested against synthetic `LogLineDto[]` (mock IPC), so it builds independently.
- **Behavior:** bottom strip, resizable (default 120px, min 60, max 50% height; persisted). Header: session state (Idle/Connecting/In-session/Disconnecting) + `[Human | Raw]` toggle + Copy button. **Human projection (default):** surface operator-relevant lines (`***`-annotated + `LogSource::Backend`/`Transport` info) + a per-session summary; suppress raw B2F (`;PQ`,`;PR`,`[WL2K-...]`,`;FW`,`FF`,`FQ`, `LogSource::Wire`). **Raw projection:** everything. Both are projections of the same `LogLineDto[]` — NO parallel streams. Live tail auto-scroll; pause on scroll-up; resume on scroll-to-bottom or new session.
- **State:** `lines: LogLineDto[]`, `projection: 'human'|'raw'`, `stuckToBottom` ref. `logProjection.ts` is a pure function — the prime unit-test target.

### 5.6 Task 16 — Dashboard ribbon + minimal status bar (`tuxlink-hvv`) [independent build; Wave 2A]

Per AMD-8 — TWO surfaces.

- **Files (create):** `src/shell/DashboardRibbon.tsx`, `src/shell/StatusBar.tsx`, `src/shell/useStatus.ts` (+ tests). **Modify:** `ui_commands.rs` (append `config_read` + `backend_status` command fns — `config_read` reads `config.rs` with no AppBackend dependency, keeping Task 16 independent for build). **Does NOT edit `AppShell.tsx` or `lib.rs`'s `invoke_handler`** — ribbon/statusbar region wiring + command registration land in the orchestrator integration commit (§4.3).
- **Behavior — Dashboard ribbon** (top, ~40px, always visible): Callsign (from `identity.callsign`, or `identity.identifier` offline) · Grid (4-char broadcast; tooltip 6-char if `position_precision==SixCharGrid`) · GPS status (on/manual/off/searching from `privacy.gps_state`) · UTC + local time · Connection state with transport always named. **Connection-state consumes live `status()` when the backend exists, else config/offline** (Codex verdict 6 — `status()` is sync/non-I/O per the trait at `winlink_backend.rs:250`, so it is cheap to poll): the `backend_status` command returns `BackendStatus` (which already names the transport — `Connected{transport, peer, since_iso}`) when `AppBackend` is populated; when `None` (offline install or pre-connect), fall back to a config-derived "Idle · <configured CmsTransport>". Richer than a pure stub, and Task 16 stays independent for build (the ribbon's formatters are pure-function unit tests against synthetic `BackendStatus`; the `backend_status` command is registered in the integration commit). **Status bar** (bottom, ~24px, toggleable via View→Toggle Status Bar): left = app activity; right = window-info. Mail.app-minimal, NOT Express cryptic-strip.
- **State:** `config_read` query (5s); `useStatus` returns the config-derived snapshot. `formatStatus`/ribbon-format are pure functions — the unit-test targets.

---

## 6. Test lists (5–10 per task; static tests verify model/logic, NOT rendered widgets — operator smoke at M2 is the runtime gate per `testing-pitfalls.md` §9)

**Task 8 (tray, Rust):** (1) `tray_event_ids()` contains `tray:show_hide`/`tray:new_message`/`tray:quit`. (2) menu builds without panic. (3) close-handler hides not exits (assert `prevent_close` path — structural). (4) Quit handler calls `app.exit(0)` (not PredefinedMenuItem). *(Runtime hide/show verified at M2.)*

**Task 12 (model + list + bootstrap):** (1) `mailbox_list` maps `MessageMeta`→DTO incl. `to`+`hasAttachments` (mockito Pat fixture). (2) folder string parse: `"drafts"`/`"deleted"` rejected/handled. (3) `MessageList` renders subject/from/to/size for a row. (4) empty-folder → empty-state copy. (5) unread row gets `unread` class. (6) selecting a row calls `onSelect(id)` and does NOT remount the shell (selection-state, not route). (7) `read_message_in(Inbox,id)` == old `read_message(id)` (back-compat). (8) `AppBackend` `None` → `mailbox_list` returns `NotConfigured` projection. (9) FolderSidebar renders Deleted/Templates disabled.

**Task 13 (reading + parse):** (1) RFC5322 parse extracts subject/from/to/cc/date/body from a fixture. (2) multipart message → attachment names listed, text/plain body decoded. (3) form payload (`<?xml`) → `isForm true`. (4) non-UTF-8 bytes → lossy decode, no panic. (5) `message_read` on missing MID → `NotFound`. (6) MessageView header strip shows routing when present, omits when null. (7) no-selection → "Select a message."

**Task 14 (compose):** (1) draft round-trips localStorage. (2) `clearDraft` removes. (3) `loadDraft` unknown→null. (4) To/Cc split on `;` trim empties. (5) send maps to `OutboundDraftDto` (to+cc+subject+body). (6) `Ok(None)` from send → "Posted to Outbox" success (not error). (7) close-with-changes prompts (structural). (8) autosave fires after 2s (fake timers).

**Task 15 (log projection):** (1) Human projection keeps `***`/Backend/Transport, drops Wire/`;PQ`/`;PR`/`FF`/`FQ`. (2) Raw keeps everything. (3) both read same input array (no dual stream). (4) empty input → empty render. (5) summary line derived per session. (6) auto-scroll pauses when `stuckToBottom=false` (ref logic). (7) `LogLineDto` level/source enums round-trip.

**Task 16 (ribbon + status):** (1) `formatStatus` idle. (2) connection-state names configured transport (CMS-SSL/Telnet). (3) ribbon shows callsign from `identity.callsign`, falls back to `identity.identifier` offline. (4) grid shows 4-char; tooltip 6-char only when `SixCharGrid`. (5) GPS status maps each `gps_state`. (6) `config_read` shape parses. (7) status bar hidden when toggled off.

---

## 7. Cross-task file-ownership map

| File | Owner | Consumers (read-only import) |
|---|---|---|
| `src/mailbox/types.ts` (MessageMeta, ParsedMessage, UiError, MailboxFolder) | **Task 12** | 13, 14 |
| `src/shell/AppShell.tsx` | **Task 12** creates (with placeholders); **orchestrator integration commit** swaps placeholders → real components (§4.3) | — |
| `src/App.tsx` (wizard→shell routing) | **Task 12** creates; integration commit adds main-window-only `menu:file:new` listener | — |
| `src-tauri/src/winlink_backend.rs`, `pat_client.rs` (MessageMeta fields, `read_message_in`) | **Task 12** | 13 (read) |
| `src-tauri/src/app_backend.rs` + bootstrap (in `lib.rs`'s `run()`) + `session_log:line` emission | **Task 12** | 13, 14, 15 |
| `src-tauri/src/ui_commands.rs` (`UiError`, command fns) | **Task 12** creates (`UiError` + `mailbox_list`); 13/14/16 each APPEND one command fn | no `invoke_handler` edit |
| `lib.rs` `invoke_handler` registration (all UI commands) + AppShell wiring | **orchestrator integration commit** (§4.3) | the single shared-file edit |
| `src/mailbox/MessageView.tsx`, `useMessage.ts` | Task 13 | — |
| `src/compose/*`, `compose_window.rs` | Task 14 | (Task 12 sidebar reads `useDraft.listDraftIds()` for Drafts) |
| `src/session/SessionLog.tsx`, `logProjection.ts` | Task 15 | — |
| `src/shell/DashboardRibbon.tsx`, `StatusBar.tsx`, `useStatus.ts` | Task 16 | — |
| `src-tauri/src/tray.rs` | Task 8 | — |

**`ui_commands.rs` + `invoke_handler` shared-edit risk:** 12/13/14/16 all add command fns. Mitigation: Task 12 creates `ui_commands.rs` with `UiError` + `mailbox_list`; 13/14/16 each APPEND one `#[tauri::command]` fn (append-only → near-zero conflict) but do NOT touch `lib.rs`'s `invoke_handler![...]`. **All `invoke_handler` registration + AppShell wiring happens in the single orchestrator integration commit (§4.3)** — eliminating the conflict hotspot rather than resolving it at each rebase. **`useDraft.ts` cross-task:** Task 14 owns it; Task 12's FolderSidebar imports `listDraftIds` for the Drafts count — so Task 12's Drafts folder is functional only after Task 14 merges; until then it shows 0 (graceful). This is an acceptable soft-dependency (no build break — `listDraftIds` is a tiny pure fn; if Task 12 needs it before 14, Task 12 stubs a local `listDraftIds` that 14 replaces).

---

## 8. New-dependency authorizations required

Per the no-new-deps-without-authorization guardrail, these are needed and must be approved (the implementing subagents add them; flagged here for the plan + operator visibility):

- **Frontend (npm):** `@tanstack/react-query` (mailbox/status polling), `react-virtuoso` (virtualized list) — Task 12. `@radix-ui/react-tabs` is **NOT** needed (folder sidebar, not tabs — drop the plan's Radix Tabs). `tauri-plugin-window-state` (compose geometry) — Task 14.
- **Rust (cargo):** an RFC5322 parser for Task 13 — `mail-parser` (pure-Rust, no_std-friendly, handles MIME multipart + encodings) preferred over `mailparse`. Task 13. **This is the one dep that warrants explicit operator/Codex scrutiny** (parsing untrusted message bytes — pick a memory-safe, maintained crate).

All are mainstream, MIT/Apache. None pulls a native toolchain beyond what Tauri already requires.

---

## 9. Resolved decisions + open items for Codex to challenge

Decisions made (converged with Codex, not surfaced to operator per `[[no-atomic-decisions-to-operator]]`):

1. **Backend access = `WinlinkBackend` trait in `AppBackend` managed state**, not raw PatClient per command. *(Challenge: is per-command `Arc` clone + `tokio::Mutex` the right concurrency shape, or should `AppBackend` hold `Arc<dyn WinlinkBackend>` directly without the outer Mutex since the trait is `Send+Sync`?)* — leaning: drop the outer Mutex, hold `RwLock<Option<Arc<dyn WinlinkBackend>>>` (set once at bootstrap, read by commands).
2. **Drafts = local; Deleted/Templates = disabled placeholders.** Real delete needs a trait `delete_message`/`move_message` — **deferred to v0.1** (named here; no v0.0.1 owner). *(Challenge: is a disabled Deleted folder acceptable UX, or map Deleted→Archive read-only?)*
3. **RFC5322 parsed at the `message_read` command boundary** (Task 13), returning `ParsedMessageDto`; raw bytes never reach the frontend. *(Challenge: parser crate choice; form-detection heuristic robustness.)*
4. **`read_message_in(folder, id)` trait addition** (vs `ReadOptions` struct or a folder field on `MessageId`). *(Challenge: simplest stable signature.)*
5. **Shell composition via per-region wiring rebased on Task 12** (§4.3). *(Challenge: would a single dedicated integration commit after Wave 2C be cleaner than 3 per-region wirings?)*
6. **Task 16 connection-state is config-derived/stub in v0.0.1** (live `BackendStatus` deferred). *(Challenge: is this too thin — should the ribbon consume live `status()` now, accepting a Task-12 dependency?)*
7. **`MessageMeta.to`/`has_attachments` additions** depend on Pat 1.0.0 actually exposing To + attachment metadata in the list DTO. *(Challenge: verify against Pat's real `/api/mailbox` JSON; if absent, `to`/`has_attachments` degrade — confirm the degradation is acceptable.)*

---

## 10. Out of scope (this cluster / v0.0.1)

- **Task 16.5 Radio Dock** — the grid reserves the `dock` column but renders nothing; no bd issue in this run.
- **Task 6** (live-CMS), **Tasks 17/18/19** (AppImage/README/CI).
- Attachment download/preview, Winlink form rendering, multi-callsign, ICS-309, GPS device integration, spell-check, message delete/move, live session-state tracking — all v0.1+ (design doc §8).

---

## 11. Codex adrev disposition (round 1, 2026-05-19)

Cross-provider round (`dev/adversarial/2026-05-19-main-ui-cluster-codex.md`). Codex read the source and labeled VERIFIED-vs-INFERRED. **All findings accepted.**

| # | Finding | VERIFIED? | Disposition |
|---|---|---|---|
| F1 | Selection state must carry `{folder, id}`, not bare `id`, or the Inbox-only bug recurs | logic | **Applied** — §4.2 `selectedMessage:{folder,id}`; §5.3 `useMessage(folder,id)` key `[folder,id]` |
| F2 | Commands register in `lib.rs` (`run()`), not `main.rs` (a 4-line shim) | ✅ `lib.rs:29`, `main.rs:4` | **Applied** — §1.1 + §5.1/§5.2/§5.4/§7 corrected to `lib.rs` |
| F3 | `MessageMeta.to`/`has_attachments` absence VERIFIED; Pat *support* INFERRED | ✅ absence; ⚠ Pat support unverified | **Applied** — §2.1 softened; graceful degradation mandatory; confirm vs real Pat fixture |
| F4 | `read_message` Inbox-hardcoded; `read_message_in` is the right fix | ✅ `winlink_backend.rs:397` | **Validated** — kept; ties to F1 |
| F5 | `send_message` returns `None`; `cc` silently dropped by Pat | ✅ `wb.rs:414/417`, `pat_client.rs:119` | **Applied** — §3.2 + §5.4: wire `cc` (verify Pat) OR disable it; never drop |
| F6 | `UiError` omits `InvalidSession`/`Cancelled`/`NotImplemented`/`Io` | ✅ `winlink_backend.rs:167` | **Applied** — §3.1 exhaustive mapping table; `config_read`→Internal |
| F7 | Compose menu listener must be main-window-only (events broadcast) | ✅ `menu.rs:123` | **Applied** — §5.4 + §4.3: main-window-only `menu:file:new`; compose windows don't listen |
| V1 | Backend state → `RwLock<Option<Arc<…>>>`, clone+drop+await | ✅ trait `Send+Sync` | **Applied** — §1.1 |
| V2 | Drafts local; Deleted/Templates disabled (don't map Deleted→Archive) | — | **Accepted** — §2.2 unchanged |
| V3 | RFC5322 parse: add size limit + parser-failure UI state | — | **Applied** — §5.3 |
| V5 | Shell composition: prefer ONE integration commit over per-region rebases | — | **Applied** — §4.3 rewritten; §7 ownership updated |
| V6 | Task 16 status too thin — consume live `status()` w/ config fallback | ✅ `status()` sync `wb.rs:250` | **Applied** — §5.6 |

No findings deferred or rejected. The round corrected one factual error (F2) and one data-loss footgun (F5) that single-provider review would likely have missed.

---

**End of spec.** Plan of record that implements this: `docs/superpowers/plans/2026-05-19-main-ui-cluster-plan.md`.

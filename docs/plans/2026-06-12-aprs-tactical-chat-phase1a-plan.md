# APRS Tactical Chat — Phase 1a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Send and receive text APRS messages to/from a single callsign over the UV-Pro Bluetooth KISS transport, with honest `sent → ACKed → timed-out` delivery states, a default `WIDE1-1,WIDE2-1` digipeater path, and mandatory duplicate suppression — surfaced as a basic conversation thread inside the tuxlink workspace.

**Architecture:** A new `winlink/aprs/` Rust module sits above the existing `winlink/ax25/` transport. It adds (1) an AX.25 **UI-frame** codec (`Control::Ui`, which the enum lacks today), (2) an APRS message-format codec pinned to direwolf/aprslib source, and (3) an **actor task** (`AprsEngine`) that owns the KISS byte-link, runs a **promiscuous** RX loop (the existing `recv_frame` dest-filters and `answer()` waits for SABM — neither works for APRS), serializes all TX through one mpsc queue with a bounded retransmit schedule and a single global abort, dedupes inbound frames, and bridges to the React UI via Tauri events (`app.emit` → `listen`, the same pattern `b2f-event` uses). The frontend adds an inline chat panel (no pop-up windows).

**Tech Stack:** Rust (Tauri backend, `winlink/ax25` reuse: `kiss.rs`, `frame.rs` `Address`/`Path`, `link.rs` `connect_link_with_abort`), React/TypeScript (lazy panel in `AppShell`, `@tauri-apps/api/event` listener hook), serde config.

---

## Build & verification posture (READ FIRST — applies to EVERY task)

**Rust is CI-only on this machine.** Do NOT run `cargo build`/`cargo test`/`cargo clippy` locally — this Pi is contended and cold cargo builds never finish (memory `feedback_no_cold_cargo_on_contended_pi`). For Rust tasks:
- Write the test and the implementation. Do NOT attempt to run cargo to see it fail/pass. State in your report "Rust — CI-only, not run locally."
- The **orchestrator** (not the implementer subagent) commits each task from the worktree and lets **GitHub CI on the draft PR** compile both arches + run `cargo clippy --all-targets --locked -D warnings` + the test target. CI is the green gate.
- Subagents working in the worktree **cannot commit** (their Bash cwd resets to the main checkout each call, and the main-checkout-race hook denies the commit — memory `feedback_subagents_cannot_commit_in_worktrees`). Implementer subagents: write code + tests, leave the tree dirty, STOP. The orchestrator commits via a standalone `cd <worktree>` call then `git commit` in the next call.
- Write clippy-clean code proactively (CI is the only clippy gate): no `..Default::default()` when all fields are specified (`needless_update`), `#[cfg(test)]` impls before `mod tests` (`items_after_test_module`), collapse chained `if let ... else { return None }` to `?` (`question_mark`), derive `Debug` on any type a test passes to `.expect_err()`/`.unwrap_err()`. Re-run history: managed Dire Wolf lost 4 CI rounds to exactly these.

**Frontend gates run locally and are cheap** — DO run them:
- `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat tsc --noEmit` (typecheck)
- Scoped vitest: `pnpm -C <worktree> vitest run src/aprs/<file>.test.tsx` — but reap zombies after (`ps -eo pid,args | grep '[n]ode' | grep vitest`), never `pkill -f vitest` (self-match kills the run — memory `vitest-pkill-self-match`). Modern vitest cleans its own pool on normal exit; only reap if a run was interrupted.

**Pin paths** — always pass `pnpm -C <abs-worktree>` and `cargo --manifest-path <abs-worktree>/src-tauri/Cargo.toml`; the Bash cwd silently reverts to the main checkout mid-session (memory `pin_paths_in_worktree_sessions`).

**TDD preamble for every task** (the implementer subagent must do this):
```
BEFORE starting work:
1. Read .claude/skills/test-driven-development/ (or invoke /test-driven-development)
2. Read docs/pitfalls/testing-pitfalls.md and docs/pitfalls/implementation-pitfalls.md
Follow TDD: write failing test → implement → verify green (frontend: run vitest; Rust: CI verifies).
```
**Completion check for every task:**
```
BEFORE marking complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md (error paths? edge cases?)
2. Frontend: run scoped vitest + tsc, confirm green. Rust: confirm it compiles in your head + state CI-only.
3. Leave the worktree dirty + uncommitted; report DONE so the orchestrator commits.
```
**After every logical group of tasks** (the orchestrator does this between tasks via the two-stage review): minimum three review rounds from multiple perspectives; if substantive issues remain at round three, keep going.

---

## RADIO-1 (transmit discipline — governs Tasks 7–11)

APRS messaging transmits: every `aprs_send` keys the radio. Per ADR 0018, the **agent never runs a transmit-capable binary against real hardware** — the operator's on-air UV-Pro smoke is the validation. The agent freely writes/tests this TX code (mocks/fakes), but the correctness bar is **working abort + no runaway TX**:

- **Bounded airtime by construction.** APRS UI frames are short, discrete, fire-and-forget — no connected-mode runaway. The one unbounded risk is the message-ACK **retransmit loop**. Task 8 pins an **exact** retry schedule (a small bounded count), a **single serialized TX queue** (all sends through one mpsc → one actor → one writer; this is standard APRS-client behavior, NOT a tuxlink-added safeguard), a **cap on concurrent in-flight messages**, and a **single global abort** that flushes ALL pending retransmit timers before the next TX.
- **No tuxlink-added safeguards** beyond standard APRS behavior (memory `feedback_no_tuxlink_added_safeguards`): the retry cap is standard APRS, not a tuxlink invention. Do NOT add airtime caps, TOT timers, or extra confirmation modals beyond the Send click.
- **Worst-case airtime arithmetic** (state it in the Task 8 doc-comment): `concurrent_cap` messages × (`1 initial + N retries`) transmissions × `frame_airtime`. With cap=8, retries=3, that is 8 × 4 = 32 frames over the retry window; at ~0.5 s/frame on 1200 baud ≈ 16 s aggregate keying spread across the window, never a continuous key-down.

---

## Single-Bluetooth-host arbitration (scope note — NOT a task)

The UV-Pro is a single RFCOMM host and a single radio. The `AprsEngine` holds the BT KISS link while listening; the existing Winlink `packet_connect`/`packet_listen` path also opens that same link. **They cannot both hold it at once.** Phase 1a does NOT build cross-arbitration — the operator uses one at a time (APRS chat OR Winlink-over-packet). If `aprs_listen_start` is called while a packet session holds the link, `connect_link_with_abort` will fail to open the RFCOMM socket; surface that as a named error ("radio link is in use by the packet session"), do not crash. Full arbitration is a Phase 1b concern. Document this in the `aprs_listen_start` command and the panel's empty state.

---

## File structure

**New Rust module — `src-tauri/src/winlink/aprs/`:**
- `mod.rs` — module surface + re-exports; `pub mod` declarations.
- `message.rs` — APRS message-format codec: `encode_message`, `encode_ack`, `encode_rej`, `parse_info` → `AprsPayload` enum. Pure, fixture-tested (the 11 vectors). **Bug-prone — exact field widths.**
- `identity.rs` — `AprsIdentity { source: Address, tocall: Address, path: Vec<Address> }` + `parse_path("WIDE1-1,WIDE2-1")`. Resolution from config + active Winlink base call.
- `dedupe.rs` — `DedupeCache` keyed by `(src CALL-SSID, payload-kind, msgid)` with a time window. Clock injected for tests.
- `tx.rs` — `TxQueue` retransmit state machine (pure, injected clock): pending map, retry schedule, concurrent cap, `tick(now)`, `on_ack`, `abort`. **The RADIO-1 bound — fully unit-tested, no I/O.**
- `framebuild.rs` — build an APRS UI `Frame` from `AprsIdentity` + dest call + info bytes; reuse `Path::encode`/`Frame::encode`. Parse inbound `Frame` → sender `Address` + info.
- `engine.rs` — `AprsEngine` actor: owns the `Box<dyn ByteLink>`, RX loop (promiscuous), wires dedupe/tx/framebuild, auto-acks inbound, emits events. Integration-tested against an in-memory `FakeLink`.

**Modified Rust files:**
- `src-tauri/src/winlink/ax25/frame.rs` — add `Control::Ui { pf: bool }` (Task 1).
- `src-tauri/src/winlink/ax25/mod.rs` — no change needed (frame re-exports already cover `Control`); verify `Control` is `pub`.
- `src-tauri/src/winlink/mod.rs` — add `pub mod aprs;`.
- `src-tauri/src/config.rs` — add `[aprs]` `AprsConfig` section (Task 5).
- `src-tauri/src/ui_commands.rs` — `aprs_*` commands + DTOs (Task 11).
- `src-tauri/src/lib.rs` — register commands; add `AprsState` managed state (Tasks 10–11).

**New frontend files — `src/aprs/`:**
- `aprsTypes.ts` — wire DTOs (Task 12).
- `useAprsChat.ts` — event-listener hook + thread state (Task 12).
- `AprsChatPanel.tsx` — the inline chat surface (Task 13).
- `AprsChatPanel.test.tsx`, `useAprsChat.test.ts` — vitest.

**Modified frontend files:**
- `src/shell/AppShell.tsx` — lazy-mount the panel inline (Task 14).
- `src/aprs/AprsSettings.tsx` (new) + wherever SettingsPanel composes sections — APRS identity settings (Task 14).

### Cross-cutting conventions (every task obeys these)

**`pub mod` discipline.** Every task that CREATES an `aprs/*.rs` file also adds its `pub mod <name>;` line to `src-tauri/src/winlink/aprs/mod.rs` (Task 2 creates `mod.rs` itself + `pub mod aprs;` in `winlink/mod.rs`). A task that creates a file but forgets its `pub mod` line breaks the build for every later task. There is no `aprs/state.rs` — `AprsState` lives in `engine.rs` (Task 10), so no extra mod line there.

**One `mod tests` per file.** `message.rs` is touched by Tasks 2/3/4. Task 2 creates the `#[cfg(test)] mod tests { use super::*; … }` block; Tasks 3 and 4 **append their `#[test] fn`s INTO that existing block** — do NOT open a second `mod tests` (duplicate-module compile error). Same rule for any file a later task extends.

**Assumed reuse signatures** (the grounding reviewer verified these against the real code; if an implementer finds a mismatch, STOP and flag — do not guess): `kiss::kiss_data_frame(&[u8]) -> Vec<u8>`; `kiss::KissDecoder::new()` + `KissDecoder::push(&mut self, &[u8]) -> Vec<Vec<u8>>`; `ax25::link::ByteLink: Read + Write + Send`; `ax25::connect_link_with_abort(&KissLinkConfig, Arc<AtomicBool>) -> io::Result<(Box<dyn ByteLink>, Option<TcpStream>)>` (re-exported at `crate::winlink::ax25::`); `frame::{Address{call,ssid}, Path{dest,src,digis}, Frame{path,control,info}, Control}` all `pub`, `Control` derives `Debug+Clone+PartialEq+Eq`; `Frame::encode(&self) -> Result<Vec<u8>, FrameError>` / `Frame::decode(&[u8]) -> Result<Frame, FrameError>` gate PID+info purely on `control.has_info()`.

---

## Authoritative APRS wire format (grounded in source — paste-reference for Tasks 2–4, 7)

Sources: direwolf `encode_aprs.c` (`aprs_message_t` struct + `encode_message`), direwolf `decode_aprs.c` (`aprs_message()`), aprslib `parsing/message.py`. **Implementations win over prose.**

**Message info field** (the bytes AFTER the AX.25 PID 0xF0):
```
:ADDRESSEE:message text{XXXXX
│└───9────┘│           │└msgID (optional, 1–5 alnum)
│         2nd colon    │
DTI ':'   (offset 10)  '{' delimiter
```
- DTI: literal `:` (offset 0).
- Addressee: **exactly 9 chars, left-justified, SPACE-padded (0x20)**, then a second literal `:`. Fixed 11-byte prefix. Encoder proof: `memset(addressee,' ',9); memcpy(addressee, call, min(len,9))`.
- Message text: emit **≤67 chars** (spec); on RECEIVE accept arbitrary length (direwolf & aprslib both over-accept). Text MUST NOT contain `{` (msgID delimiter).
- msgID: optional, starts at `{`, **1–5 alphanumeric** chars, at the very end.

**ACK:** `:SENDER   :ackNNNNN` — addressee = the **original sender** (the station you received the message from), space-padded to 9; `ack` literal **lowercase**; `NNNNN` = the original message's msgID **echoed verbatim** (never your own counter). **REJ:** identical with `rej`.

**Reply-ack (new format, 1999) — TOLERATE on parse, do NOT emit in 1a:** detect a `}` at msgID offset 2 (`{MM}` or `{MM}AA`, and `ackMM}` / `ackMM}AA`). Strip from the first `{` for display; extract the 2-char msgID; ignore the trailing `}AA`.

**AX.25 UI-frame wrapping:** control = `0x03`, PID = `0xF0`. Address path = dest (tocall) → src (sender CALL-SSID) → digis (`WIDE1-1,WIDE2-1`). **tocall = `APZTUX`** (confirmed valid + unallocated experimental `APZ` range; do NOT use generic `APRS`). C-bit convention: set **dest C-bit=1, src C-bit=0** (spec-correct command-frame; `Path::encode` already does exactly this) and **ignore both C-bits on receive** — the APRS ecosystem ignores them.

**Test vectors (11) — full info fields. Count the addressee spaces.**
| # | Info field (with leading `:`) | Meaning | Source |
|---|---|---|---|
| 1 | `:WXBOT    :HelloWorld  ` | plain msg, no msgID | aprslib |
| 2 | `:WXBOT    :HelloWorld  {ABCDE` | msg + 5-char msgID | aprslib |
| 3 | `:WXBOT    :rej123` | REJ of `123` | aprslib |
| 4 | `:WXBOT    :ackAB}` | new-style ACK of `AB` | aprslib |
| 5 | `:WXBOT    :ackAB}CD` | new ACK of `AB`, piggyback `CD` | aprslib |
| 6 | `:WXBOT    :HelloWorld  {AB}CD` | reply-ack msg, msgID `AB`, piggyback `CD` | aprslib |
| 7 | `:WA1XYX-15:Howdy y'all` | plain msg (addressee exactly 9, no pad) | direwolf |
| 8 | `:WA1XYX-15:Howdy y'all{12345` | msg + 5-char msgID | direwolf |
| 9 | `:WA1XYX-15:Howdy y'all{12}` | new-style msgID `12` | direwolf |
| 10 | `:WA1XYX-15:Howdy y'all{12}34` | reply-ack msgID `12` piggyback `34` | direwolf |
| 11 | `:N2GH     :some stuff` | direwolf encoder self-test output | direwolf |

---

## Task 1: `Control::Ui` variant in the AX.25 frame codec

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs` (the `Control` enum ~126–180, `has_info` ~176–179, `Frame::decode` ~329–344, `Frame::encode` ~346–357)
- Test: same file, `#[cfg(test)]` module

**Context:** The `Control` enum today has SABM/DISC/UA/DM/RR/RNR/REJ/I — **no UI variant**. `Control::decode(0x03)` returns `Err(UnknownControl(0x03))`. `has_info()` is true only for `I`. `Frame::decode` reads+discards a PID byte only when `control.has_info()`, and `Frame::encode` emits PID `0xF0` only for info frames. A UI frame (`0x03`, possibly with P/F bit `0x10`) carries a PID + info exactly like an I-frame. We add `Ui { pf }` and make the info/PID path apply to it. Do NOT change the I-frame logic.

- [ ] **Step 1: Write the failing tests** (append to the `#[cfg(test)]` module in `frame.rs`)
```rust
#[test]
fn control_ui_encodes_to_0x03() {
    assert_eq!(Control::Ui { pf: false }.encode(), 0x03);
    assert_eq!(Control::Ui { pf: true }.encode(), 0x13); // P/F bit 0x10
}

#[test]
fn control_ui_decodes_from_0x03() {
    assert_eq!(Control::decode(0x03).unwrap(), Control::Ui { pf: false });
    assert_eq!(Control::decode(0x13).unwrap(), Control::Ui { pf: true });
}

#[test]
fn control_ui_has_info() {
    assert!(Control::Ui { pf: false }.has_info());
}

#[test]
fn ui_frame_round_trips_with_pid_and_info() {
    let path = Path {
        dest: Address { call: "APZTUX".into(), ssid: 0 },
        src: Address { call: "N0CALL".into(), ssid: 9 },
        digis: vec![],
    };
    let info = b":N0CALL   :hi{01".to_vec();
    let f = Frame { path: path.clone(), control: Control::Ui { pf: false }, info: info.clone() };
    let bytes = f.encode().unwrap();
    let decoded = Frame::decode(&bytes).unwrap();
    assert_eq!(decoded.control, Control::Ui { pf: false });
    assert_eq!(decoded.info, info);
}
```
NOTE: confirm `Control` derives `PartialEq, Debug, Clone` — if not, add the derives (needed for `assert_eq!`). Confirm `Path`, `Address`, `Frame` are constructible from the test module (they are `pub` per the grounding).

- [ ] **Step 2: (CI verifies — do NOT run cargo)** State expected: FAIL — `Ui` variant does not exist.

- [ ] **Step 3: Add the `Ui` variant + wire it through**

In the `Control` enum, add:
```rust
    Ui { pf: bool },   // AX.25 Unnumbered Information frame (APRS rides these)
```
In `Control::encode()`, add to the U-frame arm (UI control = `0x03`, P/F bit = `0x10`):
```rust
    Control::Ui { pf } => 0x03 | if *pf { 0x10 } else { 0 },
```
In `Control::decode(b)`, in the U-frame match (after masking off the P/F bit `0x10`), add the `0x03` pattern:
```rust
    0x03 => Ok(Control::Ui { pf }),
```
(Place it alongside the existing `0x2F => SABM`, `0x43 => DISC`, etc. arms, using the same already-extracted `pf`.)
In `has_info()`:
```rust
    pub fn has_info(&self) -> bool {
        matches!(self, Control::I { .. } | Control::Ui { .. })
    }
```
`Frame::decode` and `Frame::encode` already gate the PID+info path on `control.has_info()`, so adding `Ui` to `has_info` makes them carry PID `0xF0` + info for UI frames automatically. **Verify** the encode/decode PID handling has no `I`-only special-casing beyond `has_info` — if `Frame::encode` matches on the control variant anywhere, extend it to treat `Ui` like `I` for the PID/info push.

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (uncommitted; orchestrator commits as `feat(aprs): add AX.25 UI-frame control variant for APRS`).

---

## Task 2: APRS message ENCODE (`:ADDRESSEE:text{id`)

**Files:**
- Create: `src-tauri/src/winlink/aprs/message.rs`
- Create: `src-tauri/src/winlink/aprs/mod.rs` (add `pub mod message;`)
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod aprs;`)
- Test: in `message.rs`

**Context:** Produce the APRS info-field byte string for an outgoing message. Fixed 11-byte prefix `:` + 9-char-padded addressee + `:`, then text, then optional `{msgid`. Pin to direwolf `encode_aprs.c`: space-pad, left-justify, truncate addressee to 9; truncate text to 67.

- [ ] **Step 1: Write the failing tests** (`message.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_message_pads_addressee_to_9_and_appends_msgid() {
        // direwolf encoder self-test: encode_message("N2GH","some stuff","") => ":N2GH     :some stuff"
        assert_eq!(encode_message("N2GH", "some stuff", None), b":N2GH     :some stuff".to_vec());
    }

    #[test]
    fn encode_message_with_msgid() {
        assert_eq!(encode_message("WA1XYX-15", "Howdy y'all", Some("12345")),
                   b":WA1XYX-15:Howdy y'all{12345".to_vec());
    }

    #[test]
    fn encode_message_truncates_text_to_67() {
        let long = "x".repeat(80);
        let out = encode_message("AB", &long, None);
        // prefix ":AB       :" = 11 bytes, then exactly 67 x's
        assert_eq!(out.len(), 11 + 67);
    }

    #[test]
    fn encode_message_truncates_addressee_to_9() {
        let out = encode_message("VERYLONGCALL", "hi", None);
        assert_eq!(&out[..11], b":VERYLONGC:"); // 9 chars of the call, no padding needed
    }
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL — `encode_message` undefined.

- [ ] **Step 3: Implement**
```rust
/// Build an APRS message info field: `:ADDRESSEE:text{msgid`.
/// addressee is left-justified, space-padded to exactly 9, truncated at 9.
/// text is truncated to the APRS 67-char limit. msgid (if present) is appended after `{`.
pub fn encode_message(addressee: &str, text: &str, msgid: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b':');
    out.extend_from_slice(&pad_addressee(addressee));
    out.push(b':');
    let text: String = text.chars().take(67).collect();
    out.extend_from_slice(text.as_bytes());
    if let Some(id) = msgid {
        out.push(b'{');
        out.extend_from_slice(id.as_bytes());
    }
    out
}

/// 9-byte addressee: left-justified, space-padded, truncated at 9.
fn pad_addressee(call: &str) -> [u8; 9] {
    let mut buf = [b' '; 9];
    let bytes = call.as_bytes();
    let n = bytes.len().min(9);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf
}
```

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (orchestrator commits `feat(aprs): APRS message-field encoder`).

---

## Task 3: APRS info-field PARSE (message / ack / rej, tolerate reply-ack)

**Files:**
- Modify: `src-tauri/src/winlink/aprs/message.rs`
- Test: same file — **append the new `#[test] fn`s INTO the existing `#[cfg(test)] mod tests` block created in Task 2; do NOT open a second `mod tests`** (duplicate-module compile error).

**Context:** Parse an inbound APRS info field into a typed payload. Must handle: plain message, message+msgID, ack, rej, and **tolerate** the new reply-ack `}` forms without choking. Ground against all 11 vectors. The `pad_addressee`/trim asymmetry: on receive, trim trailing spaces off the addressee.

- [ ] **Step 1: Write the failing tests**
```rust
#[test]
fn parse_plain_message() {
    let p = parse_info(b":WXBOT    :HelloWorld  ").unwrap();
    assert_eq!(p, AprsPayload::Message {
        addressee: "WXBOT".into(), text: "HelloWorld".into(), msgid: None });
}

#[test]
fn parse_message_with_msgid() {
    let p = parse_info(b":WA1XYX-15:Howdy y'all{12345").unwrap();
    assert_eq!(p, AprsPayload::Message {
        addressee: "WA1XYX-15".into(), text: "Howdy y'all".into(), msgid: Some("12345".into()) });
}

#[test]
fn parse_ack_old_format() {
    let p = parse_info(b":WXBOT    :ack003").unwrap();
    assert_eq!(p, AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "003".into() });
}

#[test]
fn parse_rej_old_format() {
    let p = parse_info(b":WXBOT    :rej123").unwrap();
    assert_eq!(p, AprsPayload::Rej { addressee: "WXBOT".into(), msgid: "123".into() });
}

#[test]
fn parse_new_format_ack_tolerated() {
    // ackAB} and ackAB}CD both extract msgid "AB" (ignore piggyback)
    assert_eq!(parse_info(b":WXBOT    :ackAB}").unwrap(),
               AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "AB".into() });
    assert_eq!(parse_info(b":WXBOT    :ackAB}CD").unwrap(),
               AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "AB".into() });
}

#[test]
fn parse_reply_ack_message_tolerated() {
    // {AB}CD => msgid "AB", piggyback ignored; text excludes the {.. tail
    let p = parse_info(b":WXBOT    :HelloWorld  {AB}CD").unwrap();
    assert_eq!(p, AprsPayload::Message {
        addressee: "WXBOT".into(), text: "HelloWorld".into(), msgid: Some("AB".into()) });
}

#[test]
fn parse_rejects_too_short() {
    assert!(parse_info(b":short").is_none());        // < 11 byte prefix
    assert!(parse_info(b"no colon dti").is_none());  // missing leading ':'
}
```
NOTE on `text` trimming: the tests trim trailing spaces off message text ("HelloWorld  " → "HelloWorld"). Match aprslib (it rstrips). Trim trailing ASCII spaces only.

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AprsPayload {
    Message { addressee: String, text: String, msgid: Option<String> },
    Ack { addressee: String, msgid: String },
    Rej { addressee: String, msgid: String },
}

/// Parse an APRS message info field. Returns None if it is not a well-formed
/// message-type packet (wrong DTI, too short, malformed addressee field).
pub fn parse_info(info: &[u8]) -> Option<AprsPayload> {
    // Fixed prefix: ':' + 9-char addressee + ':' = 11 bytes minimum.
    if info.len() < 11 || info[0] != b':' || info[10] != b':' {
        return None;
    }
    let addressee = std::str::from_utf8(&info[1..10]).ok()?.trim_end_matches(' ').to_string();
    let body = std::str::from_utf8(&info[11..]).ok()?;

    // ack / rej (literal lowercase per direwolf). msgid = everything after the keyword,
    // truncated at a '}' (new reply-ack form) so "ackAB}CD" => "AB".
    for (kw, is_ack) in [("ack", true), ("rej", false)] {
        if let Some(rest) = body.strip_prefix(kw) {
            let msgid = trim_msgid(rest);
            if msgid.is_empty() { return None; } // direwolf errors on missing number
            return Some(if is_ack {
                AprsPayload::Ack { addressee, msgid }
            } else {
                AprsPayload::Rej { addressee, msgid }
            });
        }
    }

    // Plain message, optional {msgid (old) or {MM}AA (new — tolerate).
    let (text, msgid) = match body.split_once('{') {
        Some((t, id_tail)) => (t, Some(trim_msgid(id_tail))),
        None => (body, None),
    };
    let text = text.trim_end_matches(' ').to_string();
    let msgid = msgid.filter(|m| !m.is_empty());
    Some(AprsPayload::Message { addressee, text, msgid })
}

/// Extract a usable msgid from the tail after `ack`/`rej`/`{`: stop at a '}'
/// (new reply-ack delimiter) and cap at 5 chars (old-format max).
fn trim_msgid(tail: &str) -> String {
    let core = tail.split('}').next().unwrap_or("");
    core.chars().take(5).collect()
}
```

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): APRS info-field parser (message/ack/rej + reply-ack tolerance)`).

---

## Task 4: APRS ACK/REJ encode

**Files:**
- Modify: `src-tauri/src/winlink/aprs/message.rs`
- Test: same file — **append into the existing `#[cfg(test)] mod tests` block (Task 2); do NOT open a second one.**

**Context:** Build an outgoing ack/rej. **The addressee is the ORIGINAL SENDER** (the station whose message we are acking), space-padded to 9; the msgID is **echoed verbatim** from their message; `ack`/`rej` is literal lowercase. Reuse `pad_addressee`.

- [ ] **Step 1: Write the failing tests**
```rust
#[test]
fn encode_ack_addresses_original_sender_and_echoes_msgid() {
    // We received msg 003 from KK6XYZ; our ack is addressed back to KK6XYZ.
    assert_eq!(encode_ack("KK6XYZ", "003"), b":KK6XYZ   :ack003".to_vec());
}

#[test]
fn encode_rej_lowercase() {
    assert_eq!(encode_rej("WU2Z", "47"), b":WU2Z     :rej47".to_vec());
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**
```rust
/// Build an APRS ACK addressed to the original sender, echoing their msgid.
pub fn encode_ack(original_sender: &str, msgid: &str) -> Vec<u8> {
    encode_ack_rej(original_sender, "ack", msgid)
}

/// Build an APRS REJ addressed to the original sender, echoing their msgid.
pub fn encode_rej(original_sender: &str, msgid: &str) -> Vec<u8> {
    encode_ack_rej(original_sender, "rej", msgid)
}

fn encode_ack_rej(addressee: &str, kw: &str, msgid: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b':');
    out.extend_from_slice(&pad_addressee(addressee));
    out.push(b':');
    out.extend_from_slice(kw.as_bytes()); // literal lowercase
    out.extend_from_slice(msgid.as_bytes());
    out
}
```

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): APRS ack/rej encoder`).

---

## Task 5: APRS identity + `[aprs]` config section

**Files:**
- Create: `src-tauri/src/winlink/aprs/identity.rs` (+ `pub mod identity;` in `aprs/mod.rs`)
- Modify: `src-tauri/src/config.rs` (add `AprsConfig` + `Config::aprs` field, mirror the `[packet]` pattern at ~436–478)
- Test: `identity.rs` (path parsing) + `config.rs` (round-trip/default)

**Context:** APRS station identity is **separate from the Winlink B2F identity**. Source call = the operator's base callsign + a configurable APRS SSID (default 0, distinct from the packet SSID). tocall = `APZTUX` (fixed for 1a). TX path = `WIDE1-1,WIDE2-1` default, stored as a string, parsed to `Vec<Address>`. `Address` is from `winlink::ax25::frame`.

- [ ] **Step 1: Write the failing tests**

In `identity.rs`:
```rust
#[test]
fn parse_path_splits_wide_aliases() {
    let p = parse_path("WIDE1-1,WIDE2-1").unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[0], Address { call: "WIDE1".into(), ssid: 1 });
    assert_eq!(p[1], Address { call: "WIDE2".into(), ssid: 1 });
}

#[test]
fn parse_path_handles_no_ssid() {
    let p = parse_path("RELAY").unwrap();
    assert_eq!(p[0], Address { call: "RELAY".into(), ssid: 0 });
}

#[test]
fn parse_path_empty_is_empty_vec() {
    assert_eq!(parse_path("").unwrap(), vec![]);
}

#[test]
fn parse_path_rejects_more_than_two_digis() {
    // AX.25 Path::encode rejects >2; identity must reject early with a clear error.
    assert!(parse_path("W1-1,W2-1,W3-1").is_err());
}
```

In `config.rs` test module (mirror the existing packet-config tests):
```rust
#[test]
fn aprs_config_defaults() {
    let c = AprsConfig::default();
    assert_eq!(c.source_ssid, 0);
    assert_eq!(c.tocall, "APZTUX");
    assert_eq!(c.path, "WIDE1-1,WIDE2-1");
}

#[test]
fn aprs_config_round_trips_through_toml() {
    let c = AprsConfig { source_ssid: 7, tocall: "APZTUX".into(), path: "WIDE2-1".into() };
    let s = toml::to_string(&c).unwrap();
    let back: AprsConfig = toml::from_str(&s).unwrap();
    assert_eq!(back, c);
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**

`identity.rs`:
```rust
use crate::winlink::ax25::frame::Address;

/// Resolved APRS station identity for one TX/RX session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AprsIdentity {
    pub source: Address,      // operator base call + APRS ssid
    pub tocall: Address,      // APZTUX, ssid 0
    pub path: Vec<Address>,   // digipeater aliases, 0..=2
}

/// Parse a comma path like "WIDE1-1,WIDE2-1" into addresses. Errors if >2 digis
/// (AX.25 limit) or a token is malformed.
pub fn parse_path(s: &str) -> Result<Vec<Address>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() > 2 {
        return Err(format!("APRS path has {} digipeaters; max is 2", parts.len()));
    }
    parts.iter().map(|tok| parse_addr(tok)).collect()
}

fn parse_addr(tok: &str) -> Result<Address, String> {
    let (call, ssid) = match tok.split_once('-') {
        Some((c, s)) => (c, s.parse::<u8>().map_err(|_| format!("bad SSID in '{tok}'"))?),
        None => (tok, 0),
    };
    if call.is_empty() || call.len() > 6 {
        return Err(format!("bad callsign in '{tok}'"));
    }
    if ssid > 15 {
        return Err(format!("SSID out of range in '{tok}'"));
    }
    Ok(Address { call: call.to_uppercase(), ssid })
}
```

`config.rs` — add (mirror `PacketConfig`'s serde attributes; the file uses `#[serde(default)]` + camelCase rename on DTOs but snake_case for the on-disk config — MATCH whatever `PacketConfig` does, do not introduce a new convention):
```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AprsConfig {
    pub source_ssid: u8,
    pub tocall: String,
    pub path: String,
}

impl Default for AprsConfig {
    fn default() -> Self {
        Self { source_ssid: 0, tocall: "APZTUX".into(), path: "WIDE1-1,WIDE2-1".into() }
    }
}
```
Add `pub aprs: AprsConfig` to the `Config` struct with `#[serde(default)]`.

**BLOCKER to avoid — `Config` has NO `Default` impl and is built via exhaustive struct literals (no `..Default::default()` spread) in ~6+ files.** Adding a field makes every one a `missing field 'aprs'` compile error that CI (the only gate) will catch as a whole-task failure. This task MUST add `aprs: AprsConfig::default(),` to EVERY `Config { … }` construction site. Find them first:
```bash
grep -rn "schema_version: CONFIG_SCHEMA_VERSION" /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat/src-tauri/src
```
Known sites at plan time (verify with the grep — list may have grown): `test_helpers.rs` (`native_test_config`), `winlink_backend.rs` (×2), `bootstrap.rs`, `wizard.rs` (×2), `modem_commands.rs` (×2), `ui_commands.rs`. Add the field to each. Do NOT write "include it in `Config`'s `Default`" — there is no such impl.

**Persistence is JSON, not TOML.** The on-disk config is `serde_json` (`read_config()` reads JSON; every config.rs persistence test uses `serde_json`). Do NOT write a literal `[aprs]` TOML block anywhere. Field names stay bare snake_case (matching `PacketConfig`). The Task 5 `toml::` round-trip test is fine to keep — `toml = "0.8"` IS a dependency and the test validates the serde derives in isolation — but it is NOT the real persistence path; if you prefer, swap it to `serde_json::{to_string,from_str}` to mirror the actual path (either compiles).

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): APRS identity + [aprs] config section`).

---

## Task 6: Duplicate-frame suppression cache

**Files:**
- Create: `src-tauri/src/winlink/aprs/dedupe.rs` (+ `pub mod dedupe;`)
- Test: same file

**Context:** Digipeating + sender retransmits mean the same logical frame is heard multiple times → triplicate thread entries → reads as broken. Dedupe key = `(src CALL-SSID, payload-kind, msgid)` within a time window. ACKs dedupe too (idempotent). Messages **without** a msgID can't be keyed reliably — for those, key on `(src, kind, hash(text))`. Inject the clock (`now: Instant`-like) for deterministic tests — use a `u64` millis timestamp parameter, not `Instant::now()` inside.

- [ ] **Step 1: Write the failing tests**
```rust
#[test]
fn second_identical_within_window_is_duplicate() {
    let mut c = DedupeCache::new(30_000); // 30s window
    let key = DedupeKey { src: "N0CALL-9".into(), kind: "msg".into(), id: "01".into() };
    assert!(!c.seen(key.clone(), 1000)); // first sighting at t=1s
    assert!(c.seen(key.clone(), 5000));  // again at t=5s within window => duplicate
}

#[test]
fn identical_after_window_is_fresh() {
    let mut c = DedupeCache::new(30_000);
    let key = DedupeKey { src: "N0CALL-9".into(), kind: "msg".into(), id: "01".into() };
    assert!(!c.seen(key.clone(), 1000));
    assert!(!c.seen(key.clone(), 40_000)); // 39s later, window expired => fresh
}

#[test]
fn different_msgid_is_fresh() {
    let mut c = DedupeCache::new(30_000);
    assert!(!c.seen(DedupeKey { src: "A-1".into(), kind: "msg".into(), id: "01".into() }, 1000));
    assert!(!c.seen(DedupeKey { src: "A-1".into(), kind: "msg".into(), id: "02".into() }, 1100));
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**
```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupeKey {
    pub src: String,   // "CALL-SSID"
    pub kind: String,  // "msg" | "ack" | "rej"
    pub id: String,    // msgid, or a text hash for msgid-less messages
}

/// Time-windowed duplicate suppressor. `seen` returns true if this key was seen
/// within `window_ms` before `now_ms`; otherwise records it and returns false.
pub struct DedupeCache {
    window_ms: u64,
    last_seen: HashMap<DedupeKey, u64>,
}

impl DedupeCache {
    pub fn new(window_ms: u64) -> Self {
        Self { window_ms, last_seen: HashMap::new() }
    }

    pub fn seen(&mut self, key: DedupeKey, now_ms: u64) -> bool {
        // Opportunistic prune so the map can't grow unbounded on a busy channel.
        self.last_seen.retain(|_, &mut t| now_ms.saturating_sub(t) <= self.window_ms);
        match self.last_seen.get(&key) {
            Some(&t) if now_ms.saturating_sub(t) <= self.window_ms => {
                self.last_seen.insert(key, now_ms);
                true
            }
            _ => {
                self.last_seen.insert(key, now_ms);
                false
            }
        }
    }
}
```

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): time-windowed dedupe cache`).

---

## Task 7: APRS UI-frame builder + inbound extractor

**Files:**
- Create: `src-tauri/src/winlink/aprs/framebuild.rs` (+ `pub mod framebuild;`)
- Test: same file

**Context:** Build an APRS UI `Frame` from identity + dest call + info bytes, and extract the sender + info from an inbound `Frame`. Reuse `Frame`/`Path`/`Address`/`Control::Ui` (Task 1) + `kiss::kiss_data_frame` for the wire bytes. `Path::encode` already sets dest C-bit=1, src C-bit=0 — exactly APRS convention.

- [ ] **Step 1: Write the failing tests**
```rust
#[test]
fn build_and_decode_aprs_ui_frame_round_trips() {
    let id = AprsIdentity {
        source: Address { call: "N0CALL".into(), ssid: 9 },
        tocall: Address { call: "APZTUX".into(), ssid: 0 },
        path: vec![Address { call: "WIDE1".into(), ssid: 1 },
                   Address { call: "WIDE2".into(), ssid: 1 }],
    };
    let info = b":KK6XYZ   :hello{01".to_vec();
    let frame = build_ui_frame(&id, &info);
    let bytes = frame.encode().unwrap();

    let decoded = crate::winlink::ax25::frame::Frame::decode(&bytes).unwrap();
    // dest = tocall, src = our source, info preserved
    assert_eq!(decoded.path.dest.call, "APZTUX");
    assert_eq!(decoded.path.src.call, "N0CALL");
    assert_eq!(decoded.path.src.ssid, 9);
    assert!(matches!(decoded.control, crate::winlink::ax25::frame::Control::Ui { .. }));
    assert_eq!(decoded.info, info);
}

#[test]
fn extract_inbound_returns_sender_and_info() {
    // An inbound APRS frame from KK6XYZ-7 to APZTUX
    let inbound = crate::winlink::ax25::frame::Frame {
        path: Path {
            dest: Address { call: "APZTUX".into(), ssid: 0 },
            src: Address { call: "KK6XYZ".into(), ssid: 7 },
            digis: vec![],
        },
        control: crate::winlink::ax25::frame::Control::Ui { pf: false },
        info: b":N0CALL-9 :hi there{04".to_vec(),
    };
    let (sender, info) = extract_inbound(&inbound).unwrap();
    assert_eq!(sender, "KK6XYZ-7");
    assert_eq!(info, b":N0CALL-9 :hi there{04");
}

#[test]
fn extract_inbound_rejects_non_ui_frame() {
    let i = crate::winlink::ax25::frame::Frame {
        path: Path { dest: Address{call:"A".into(),ssid:0}, src: Address{call:"B".into(),ssid:0}, digis: vec![] },
        control: crate::winlink::ax25::frame::Control::Sabm { pf: true },
        info: vec![],
    };
    assert!(extract_inbound(&i).is_none());
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**
```rust
use crate::winlink::ax25::frame::{Address, Control, Frame, Path};
use super::identity::AprsIdentity;

/// Build an APRS UI frame: dest = tocall, src = our source, digis = path,
/// control = UI, info = the APRS message bytes (PID 0xF0 added by Frame::encode).
pub fn build_ui_frame(id: &AprsIdentity, info: &[u8]) -> Frame {
    Frame {
        path: Path {
            dest: id.tocall.clone(),
            src: id.source.clone(),
            digis: id.path.clone(),
        },
        control: Control::Ui { pf: false },
        info: info.to_vec(),
    }
}

/// Format a callsign+ssid as "CALL-SSID" (or bare "CALL" for ssid 0).
pub fn fmt_callsign(a: &Address) -> String {
    if a.ssid == 0 { a.call.clone() } else { format!("{}-{}", a.call, a.ssid) }
}

/// Extract (sender "CALL-SSID", info bytes) from an inbound UI frame.
/// Returns None for non-UI frames (connected-mode traffic — ignore it).
pub fn extract_inbound(frame: &Frame) -> Option<(String, Vec<u8>)> {
    if !matches!(frame.control, Control::Ui { .. }) {
        return None;
    }
    Some((fmt_callsign(&frame.path.src), frame.info.clone()))
}
```
NOTE: `fmt_callsign` for ssid 0 returns bare call. The inbound test uses ssid 7 → "KK6XYZ-7". The `N0CALL-9` in the addressee field is opaque text here (part of `info`), not parsed by framebuild — that's the message codec's job.

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): APRS UI-frame builder + inbound extractor`).

---

## Task 8: TX queue + bounded retransmit state machine (the RADIO-1 bound)

**Files:**
- Create: `src-tauri/src/winlink/aprs/tx.rs` (+ `pub mod tx;`)
- Test: same file

**Context:** A **pure** state machine (no I/O, injected clock as `u64` millis) that the engine drives. Holds outstanding outgoing messages, decides when to (re)transmit, when to give up (timed-out), enforces the concurrent cap, and flushes on abort. **This is where bounded airtime is proven.**

**Pinned retry schedule (Phase 1a):** initial send at enqueue, then retransmit at **+30 s, +60 s, +120 s** after the initial send (3 retries). The message is **timed-out** only once **all** sends are done AND **30 s have elapsed since the LAST actual transmission** (a grace anchored on the real send time, NOT on absolute elapsed — this protects the final retry's full ACK window against irregular ticking; the async driver in Task 10 ticks on a jittery ~50 ms cadence behind a blocking link read, so an absolute `elapsed >= 150_000` boundary can otherwise time a message out seconds after its last retry actually went out). Each `tick` sends **at most one** frame (one missed offset per tick) so a sparse tick can never skip a retransmit. Total sends are hard-capped at **1 + 3 = 4** per message regardless of tick pattern → bounded airtime. **Concurrent cap = 8** in-flight messages; `enqueue` beyond the cap returns `Err(TxError::CapacityFull)`, which the command layer surfaces as a named error (the UI must NOT show an optimistic bubble for a rejected send — see Task 9/11/12). Worst-case airtime: 8 × 4 = 32 short frames spread across the retry window — never continuous key-down. Document this arithmetic in the module doc-comment.

- [ ] **Step 1: Write the failing tests**
```rust
const SCHEDULE_MS: [u64; 3] = [30_000, 60_000, 120_000]; // re-export from impl

#[test]
fn enqueue_emits_initial_send_then_scheduled_retries() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"frame-bytes".to_vec(), 0).unwrap();
    // tick at t=0 yields the initial send
    let due = q.tick(0);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].msgid, "01");
    // no resend before +30s
    assert!(q.tick(29_000).is_empty());
    // retry at +30s
    assert_eq!(q.tick(30_000).len(), 1);
    assert_eq!(q.tick(60_000).len(), 1); // +60s
    assert_eq!(q.tick(120_000).len(), 1); // +120s
}

#[test]
fn times_out_after_last_retry_window() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
    q.tick(0); q.tick(30_000); q.tick(60_000); q.tick(120_000);
    let timed = q.tick(150_000);
    assert!(timed.is_empty()); // nothing left to send
    assert_eq!(q.take_timed_out(), vec!["01".to_string()]);
}

#[test]
fn ack_removes_pending_and_reports_acked() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
    q.tick(0);
    assert!(q.on_ack("01"));        // matched
    assert!(q.tick(30_000).is_empty()); // no further sends
    assert_eq!(q.take_timed_out(), Vec::<String>::new());
}

#[test]
fn concurrent_cap_rejects_ninth() {
    let mut q = TxQueue::new();
    for i in 0..8 { q.enqueue(format!("{i:02}"), b"x".to_vec(), 0).unwrap(); }
    assert!(matches!(q.enqueue("99".into(), b"x".to_vec(), 0), Err(TxError::CapacityFull)));
}

#[test]
fn abort_flushes_all_pending() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
    q.enqueue("02".into(), b"x".to_vec(), 0).unwrap();
    let aborted = q.abort();
    assert_eq!(aborted.len(), 2);
    assert!(q.tick(30_000).is_empty()); // nothing resends after abort
}

#[test]
fn irregular_ticks_catch_up_one_retransmit_per_tick_never_skip() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
    assert_eq!(q.tick(0).len(), 1);       // initial send
    // A single tick lands AFTER both the +30s and +60s offsets. We must still emit
    // the catch-up retransmits one-per-tick, not collapse two offsets into one send.
    assert_eq!(q.tick(65_000).len(), 1);  // catches up the +30s retry
    assert_eq!(q.tick(65_001).len(), 1);  // then the +60s retry on the next tick
    assert_eq!(q.tick(65_002).len(), 0);  // +120s not reached yet → nothing due
}

#[test]
fn final_retry_keeps_full_ack_window_under_tick_jitter() {
    let mut q = TxQueue::new();
    q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
    q.tick(0);                            // initial (last_sent=0)
    q.tick(200_000);                      // very late first follow-up: catches up ONE retry, last_sent=200_000
    assert!(q.take_timed_out().is_empty()); // absolute elapsed is past 150s, but the last send was just now
    q.tick(210_000);                      // 10s since last send → still within the ACK grace
    assert!(q.take_timed_out().is_empty());
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**
```rust
//! APRS outbound TX queue with bounded retransmit (RADIO-1).
//!
//! Worst-case airtime: CONCURRENT_CAP (8) messages × (1 initial + 3 retries) = 32
//! short frames. `tick` sends AT MOST ONE frame per call and `sends_done` is hard-capped
//! at 4 per message, so total transmissions are bounded regardless of tick cadence. APRS
//! UI frames are short and discrete; there is no connected-mode key-down. A single
//! `abort()` flushes every pending retransmit timer before any further TX. The retry cap
//! is standard APRS behavior, not a tuxlink-added safeguard. Timeout is anchored on a
//! grace interval since the LAST ACTUAL SEND (not absolute elapsed) so the final retry
//! always gets its full ACK window even when the driver ticks irregularly.

/// Retransmit offsets from the initial send, in millis.
pub const SCHEDULE_MS: [u64; 3] = [30_000, 60_000, 120_000];
/// Grace after the LAST actual send before giving up (the final-retry ACK window).
const FINAL_ACK_GRACE_MS: u64 = 30_000;
/// Max simultaneously-pending outgoing messages.
pub const CONCURRENT_CAP: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub enum TxError { CapacityFull }

/// A frame the engine should transmit now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSend {
    pub msgid: String,
    pub bytes: Vec<u8>,
}

struct Pending {
    msgid: String,
    bytes: Vec<u8>,
    enqueued_ms: u64,
    sends_done: usize, // 0 = not yet sent; 1 = initial sent; 2.. = retries (capped at 1 + SCHEDULE_MS.len())
    last_sent_ms: u64, // wall position of the most recent actual (re)transmission
}

pub struct TxQueue {
    pending: Vec<Pending>,
    timed_out: Vec<String>,
}

impl TxQueue {
    pub fn new() -> Self {
        Self { pending: Vec::new(), timed_out: Vec::new() }
    }

    /// Add an outgoing message. `bytes` is the full KISS-ready frame.
    pub fn enqueue(&mut self, msgid: String, bytes: Vec<u8>, now_ms: u64) -> Result<(), TxError> {
        if self.pending.len() >= CONCURRENT_CAP {
            return Err(TxError::CapacityFull);
        }
        self.pending.push(Pending { msgid, bytes, enqueued_ms: now_ms, sends_done: 0, last_sent_ms: 0 });
        Ok(())
    }

    /// Advance the clock to `now_ms`; return frames due for (re)transmission now.
    /// Emits AT MOST ONE frame per pending message per tick (catch-up is one offset per
    /// tick — sparse ticks never skip a retransmit). A message is moved to `timed_out`
    /// only once all sends are done AND `FINAL_ACK_GRACE_MS` has elapsed since its last send.
    pub fn tick(&mut self, now_ms: u64) -> Vec<DueSend> {
        let max_sends = 1 + SCHEDULE_MS.len();
        let mut due = Vec::new();
        let mut still_pending = Vec::new();
        for mut p in self.pending.drain(..) {
            let elapsed = now_ms.saturating_sub(p.enqueued_ms);
            // How many sends SHOULD have happened by now: 1 (initial) + one per elapsed offset.
            let target_sends = 1 + SCHEDULE_MS.iter().filter(|&&off| elapsed >= off).count();
            if p.sends_done < target_sends {
                // Send exactly ONE frame this tick; the next tick advances the rest.
                due.push(DueSend { msgid: p.msgid.clone(), bytes: p.bytes.clone() });
                p.sends_done += 1;
                p.last_sent_ms = now_ms;
                still_pending.push(p);
            } else if p.sends_done >= max_sends && now_ms.saturating_sub(p.last_sent_ms) >= FINAL_ACK_GRACE_MS {
                self.timed_out.push(p.msgid);
            } else {
                still_pending.push(p);
            }
        }
        self.pending = still_pending;
        due
    }

    /// Terminate a pending message by msgid (matched inbound ACK or REJ). Returns true
    /// if it was present and removed. Idempotent: a second call for the same id is false.
    pub fn on_ack(&mut self, msgid: &str) -> bool {
        let before = self.pending.len();
        self.pending.retain(|p| p.msgid != msgid);
        self.pending.len() != before
    }

    /// Drain the list of messages that have timed out since the last call.
    pub fn take_timed_out(&mut self) -> Vec<String> {
        std::mem::take(&mut self.timed_out)
    }

    /// Flush ALL pending retransmits (single global abort). Returns the aborted msgids.
    pub fn abort(&mut self) -> Vec<String> {
        self.pending.drain(..).map(|p| p.msgid).collect()
    }
}

impl Default for TxQueue {
    fn default() -> Self { Self::new() }
}
```
NOTE: trace `times_out_after_last_retry_window` against this impl: ticks at 0/30k/60k/120k drive `sends_done` 1→2→3→4 with `last_sent_ms=120_000`; `tick(150_000)` finds `sends_done(4) >= max(4)` and `150_000 - 120_000 = 30_000 >= FINAL_ACK_GRACE_MS` → timed out, returns no DueSend. ✓ The `>=` boundaries are correct as written — do NOT loosen them. If a test fails, your `sends_done`/`last_sent_ms` accounting is wrong, not the boundary. `on_ack` is reused for REJ in Task 9 (both terminate the retransmit loop identically).

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): bounded-retransmit TX queue (RADIO-1)`).

---

## Task 9: The `AprsEngine` actor (RX loop + TX drain + event emit)

**Files:**
- Create: `src-tauri/src/winlink/aprs/engine.rs` (+ `pub mod engine;`)
- Test: same file (integration test against an in-memory `FakeLink`)

**Context:** The integration piece and the largest net-new chunk. An async task owns the `Box<dyn ByteLink>` and runs a loop that interleaves: (a) **promiscuous RX** — read bytes, `KissDecoder::push`, `Frame::decode` every frame, NO dest filter, keep only `Control::Ui` frames with a parseable APRS payload; (b) **inbound handling** — dedupe, route addressed messages to the UI, auto-ACK inbound messages that carry a msgID and are addressed to our source; (c) **TX drain** — pull `TxCommand`s off an mpsc channel, enqueue to the `TxQueue`, and on each loop `tick` the queue and write due frames; (d) **abort** — flush the queue. It emits via the host's event sink.

**Decouple from Tauri for testability:** define an `trait EventSink { fn emit_message(&self, ev: InboundMsg); fn emit_state(&self, ev: StateChange); fn emit_listening(&self, on: bool); }`. The Tauri wiring (Task 10) provides a real impl that calls `app.emit(...)`; the test provides a recording fake. Likewise abstract the link as the existing `ByteLink` (`Read + Write + Send`) so a test `FakeLink` (an in-memory VecDeque pair) drives it.

**Promiscuous decode — the crux:** do NOT call `ax25::recv_frame` (it dest-filters) or `ax25::answer` (waits for SABM). Hold the link directly, feed bytes to a `KissDecoder`, and call `Frame::decode` on each completed KISS body yourself. Filter to UI frames in the engine, not at the AX.25 layer.

- [ ] **Step 1: Write the failing integration test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::ax25::frame::{Address, Control, Frame, Path};
    use crate::winlink::ax25::kiss::kiss_data_frame;
    use std::sync::{Arc, Mutex};

    // A recording EventSink.
    #[derive(Default, Clone)]
    struct RecSink { msgs: Arc<Mutex<Vec<InboundMsg>>>, states: Arc<Mutex<Vec<StateChange>>> }
    impl EventSink for RecSink {
        fn emit_message(&self, ev: InboundMsg) { self.msgs.lock().unwrap().push(ev); }
        fn emit_state(&self, ev: StateChange) { self.states.lock().unwrap().push(ev); }
        fn emit_listening(&self, _on: bool) {}
    }

    fn identity() -> super::super::identity::AprsIdentity {
        super::super::identity::AprsIdentity {
            source: Address { call: "N0CALL".into(), ssid: 0 },
            tocall: Address { call: "APZTUX".into(), ssid: 0 },
            path: vec![],
        }
    }

    // Build the wire bytes for an inbound APRS message from KK6XYZ to N0CALL, msgid "04".
    fn inbound_message_bytes() -> Vec<u8> {
        let f = Frame {
            path: Path { dest: Address{call:"APZTUX".into(),ssid:0},
                         src: Address{call:"KK6XYZ".into(),ssid:0}, digis: vec![] },
            control: Control::Ui { pf: false },
            info: b":N0CALL   :ping{04".to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn inbound_message_is_routed_and_auto_acked() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // Feed the inbound frame; collect any frames the engine wants to transmit.
        let tx = engine.handle_inbound_bytes(&inbound_message_bytes(), 1000);
        // routed to UI
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].text, "ping");
        // auto-ack emitted as an outbound frame addressed back to KK6XYZ
        assert_eq!(tx.len(), 1);
        let decoded = Frame::decode(&strip_kiss(&tx[0])).unwrap();
        assert_eq!(decoded.path.dest.call, "APZTUX");      // our tocall
        assert_eq!(decoded.info, b":KK6XYZ   :ack04");     // ack to original sender
    }

    #[test]
    fn duplicate_inbound_suppresses_display_but_re_acks_for_lost_ack_recovery() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // First copy: displayed once + acked once.
        let tx1 = engine.handle_inbound_bytes(&inbound_message_bytes(), 1_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1);
        assert_eq!(tx1.len(), 1); // ack
        // A near-simultaneous digipeated duplicate (within the 5s ACK-throttle): NOT
        // re-displayed AND NOT re-acked — the burst collapses to one ack.
        let tx2 = engine.handle_inbound_bytes(&inbound_message_bytes(), 3_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1); // still once
        assert_eq!(tx2.len(), 0);                       // throttled, no second ack
        // The sender's RETRANSMIT ~32s later (it never saw our lost ack): still NOT
        // re-displayed (long display window), but we MUST re-ack so its recovery loop closes.
        let tx3 = engine.handle_inbound_bytes(&inbound_message_bytes(), 35_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1); // display still suppressed
        assert_eq!(tx3.len(), 1);                       // re-ack fired (past the throttle window)
    }

    #[test]
    fn inbound_rej_stops_retransmit_and_reports_rejected() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.enqueue_send("KK6XYZ", "hello", "07", 0);
        let rej = Frame {
            path: Path { dest: Address{call:"APZTUX".into(),ssid:0},
                         src: Address{call:"KK6XYZ".into(),ssid:0}, digis: vec![] },
            control: Control::Ui { pf: false },
            info: b":N0CALL   :rej07".to_vec(),
        };
        engine.handle_inbound_bytes(&kiss_data_frame(&rej.encode().unwrap()), 1000);
        assert!(sink.states.lock().unwrap().iter()
            .any(|s| s.msgid == "07" && s.state == DeliveryState::Rejected));
        // and no further retransmit goes out
        assert!(engine.tick(30_000).is_empty());
    }

    #[test]
    fn inbound_ack_transitions_outgoing_to_acked() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // queue an outgoing message id "07" to KK6XYZ
        engine.enqueue_send("KK6XYZ", "hello", "07", 0);
        // inbound ack from KK6XYZ for "07"
        let ack = Frame {
            path: Path { dest: Address{call:"APZTUX".into(),ssid:0},
                         src: Address{call:"KK6XYZ".into(),ssid:0}, digis: vec![] },
            control: Control::Ui { pf: false },
            info: b":N0CALL   :ack07".to_vec(),
        };
        engine.handle_inbound_bytes(&kiss_data_frame(&ack.encode().unwrap()), 1000);
        let states = sink.states.lock().unwrap();
        assert!(states.iter().any(|s| s.msgid == "07" && s.state == DeliveryState::Acked));
    }

    // helper: strip a single KISS data frame back to the raw AX.25 body
    fn strip_kiss(b: &[u8]) -> Vec<u8> {
        let mut d = crate::winlink::ax25::kiss::KissDecoder::new();
        d.push(b).into_iter().next().unwrap()
    }
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement** (the synchronous, testable core; the async driver wraps it in Task 10)
```rust
use crate::winlink::ax25::frame::{Address, Frame};
use crate::winlink::ax25::kiss::{kiss_data_frame, KissDecoder};
use super::dedupe::{DedupeCache, DedupeKey};
use super::framebuild::{build_ui_frame, extract_inbound, fmt_callsign};
use super::identity::AprsIdentity;
use super::message::{encode_ack, encode_message, parse_info, AprsPayload};
use super::tx::TxQueue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMsg { pub sender: String, pub text: String, pub msgid: Option<String> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState { Sent, Acked, TimedOut, Rejected }

impl DeliveryState {
    /// Terminal states release an in-flight slot (see Task 10's TauriEventSink).
    pub fn is_terminal(self) -> bool {
        matches!(self, DeliveryState::Acked | DeliveryState::TimedOut | DeliveryState::Rejected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange { pub msgid: String, pub state: DeliveryState }

pub trait EventSink: Send {
    fn emit_message(&self, ev: InboundMsg);
    fn emit_state(&self, ev: StateChange);
    fn emit_listening(&self, on: bool);
}

/// Display dedupe window (ms): suppress re-showing ANY retransmitted/digipeated copy of a
/// message for the full span a sender might retransmit it (its retry schedule + slop).
const DEDUPE_WINDOW_MS: u64 = 300_000;
/// Auto-ACK throttle window (ms): re-ACK every received copy EXCEPT collapse a burst of
/// near-simultaneous digipeated copies of one transmission into a single ACK. Shorter than
/// the sender's ~30s minimum retransmit gap, so every retransmit still re-triggers an ACK
/// (lost-ACK recovery) without an ACK storm.
const ACK_THROTTLE_MS: u64 = 5_000;

pub struct AprsEngine {
    identity: AprsIdentity,
    sink: Box<dyn EventSink>,
    decoder: KissDecoder,
    dedupe: DedupeCache,        // long window: gates UI display
    ack_throttle: DedupeCache,  // short window: rate-limits the auto-ACK
    tx: TxQueue,
}

impl AprsEngine {
    pub fn new(identity: AprsIdentity, sink: Box<dyn EventSink>) -> Self {
        Self {
            identity, sink,
            decoder: KissDecoder::new(),
            dedupe: DedupeCache::new(DEDUPE_WINDOW_MS),
            ack_throttle: DedupeCache::new(ACK_THROTTLE_MS),
            tx: TxQueue::new(),
        }
    }

    /// Feed raw bytes read from the link. Returns KISS-ready frames the caller should write
    /// back to the link (auto-acks). Routes inbound messages to the UI.
    ///
    /// Auto-ACKs are intentionally OUTSIDE the abort/TxQueue path: each is a single fire-once
    /// short frame with no retransmit timer, rate-limited by `ack_throttle`. There is no
    /// unbounded ACK loop to abort, so this is RADIO-1-safe by construction.
    pub fn handle_inbound_bytes(&mut self, bytes: &[u8], now_ms: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for body in self.decoder.push(bytes) {
            let frame = match Frame::decode(&body) { Ok(f) => f, Err(_) => continue };
            let (sender, info) = match extract_inbound(&frame) { Some(x) => x, None => continue };
            let payload = match parse_info(&info) { Some(p) => p, None => continue };
            match payload {
                AprsPayload::Message { addressee, text, msgid } => {
                    if !self.addressed_to_us(&addressee) { continue; } // 1a: only our traffic
                    // Display dedupe (long window): show each distinct message once.
                    let dkey = DedupeKey { src: sender.clone(), kind: "msg".into(),
                        id: msgid.clone().unwrap_or_else(|| text_hash(&text)) };
                    if !self.dedupe.seen(dkey, now_ms) {
                        self.sink.emit_message(InboundMsg { sender: sender.clone(), text, msgid: msgid.clone() });
                    }
                    // Auto-ACK fires on EVERY copy (idempotent lost-ACK recovery) but is
                    // throttled by a SEPARATE short window so a digipeated burst → one ACK.
                    if let Some(id) = msgid {
                        let akey = DedupeKey { src: sender.clone(), kind: "ackout".into(), id: id.clone() };
                        if !self.ack_throttle.seen(akey, now_ms) {
                            let ack = encode_ack(&sender, &id);
                            let frame = build_ui_frame(&self.identity, &ack);
                            if let Ok(b) = frame.encode() { out.push(kiss_data_frame(&b)); }
                        }
                    }
                }
                AprsPayload::Ack { addressee, msgid } => {
                    if !self.addressed_to_us(&addressee) { continue; }
                    let key = DedupeKey { src: sender, kind: "ack".into(), id: msgid.clone() };
                    if self.dedupe.seen(key, now_ms) { continue; }
                    if self.tx.on_ack(&msgid) {
                        self.sink.emit_state(StateChange { msgid, state: DeliveryState::Acked });
                    }
                }
                AprsPayload::Rej { addressee, msgid } => {
                    if !self.addressed_to_us(&addressee) { continue; }
                    // Explicit refusal — terminate the retransmit loop NOW (don't waste the
                    // 150s of airtime the timeout path would) and report a terminal state.
                    let key = DedupeKey { src: sender, kind: "rej".into(), id: msgid.clone() };
                    if self.dedupe.seen(key, now_ms) { continue; }
                    if self.tx.on_ack(&msgid) {
                        self.sink.emit_state(StateChange { msgid, state: DeliveryState::Rejected });
                    }
                }
            }
        }
        out
    }

    /// Queue an outgoing message with an ALREADY-MINTED msgid (minting happens once, upstream
    /// in `AprsState::send` — see Task 10). Capacity is also gated upstream, so the enqueue
    /// here normally succeeds; the TxQueue cap is a defense-in-depth backstop. Emits `Sent`,
    /// which means "accepted into the TX queue" (queued), NOT "keyed" — the actual frame is
    /// written by the next `tick`. (`Acked` is the only state that implies delivery.)
    pub fn enqueue_send(&mut self, dest_call: &str, text: &str, msgid: &str, now_ms: u64) {
        let info = encode_message(dest_call, text, Some(msgid));
        let frame = build_ui_frame(&self.identity, &info);
        let bytes = match frame.encode() { Ok(b) => kiss_data_frame(&b), Err(_) => return };
        if self.tx.enqueue(msgid.to_string(), bytes, now_ms).is_ok() {
            self.sink.emit_state(StateChange { msgid: msgid.to_string(), state: DeliveryState::Sent });
        }
    }

    /// Drive the retransmit clock; returns KISS-ready frames to write now. Emits TimedOut.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Vec<u8>> {
        let due: Vec<Vec<u8>> = self.tx.tick(now_ms).into_iter().map(|d| d.bytes).collect();
        for msgid in self.tx.take_timed_out() {
            self.sink.emit_state(StateChange { msgid, state: DeliveryState::TimedOut });
        }
        due
    }

    pub fn abort(&mut self) {
        for msgid in self.tx.abort() {
            self.sink.emit_state(StateChange { msgid, state: DeliveryState::TimedOut });
        }
    }

    fn addressed_to_us(&self, addressee: &str) -> bool {
        addressee == fmt_callsign(&self.identity.source)
    }
}

fn text_hash(text: &str) -> String {
    // cheap stable hash for msgid-less dedupe keying
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("h{:x}", h.finish())
}
```
NOTE: `addressed_to_us` compares against the source identity formatted as "CALL-SSID". The inbound test addresses `:N0CALL   :` (ssid 0 → bare "N0CALL"), and identity source is ssid 0 → `fmt_callsign` returns "N0CALL". ✓ If the operator runs a non-zero APRS SSID, inbound must be addressed to "CALL-SSID".

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): AprsEngine — promiscuous RX, auto-ack, TX drive`).

---

## Task 10: Async engine driver + `AprsState` managed lifecycle

**Files:**
- Modify: `src-tauri/src/winlink/aprs/engine.rs` (add the async `run` driver + a `TauriEventSink`)
- Modify: `src-tauri/src/lib.rs` (register `AprsState` via `.manage(...)`)
- Create: a small `AprsState` holder (in `engine.rs` or a new `aprs/state.rs`)
- Test: minimal — the pure core is already covered by Task 9; here add a smoke test that `AprsState::default()` constructs and `start`/`stop` flip the listening flag (no real link).

**Context:** Wrap the synchronous `AprsEngine` in a **blocking** task (run on `tokio::task::spawn_blocking`) that owns the real `Box<dyn ByteLink>` from `connect_link_with_abort`, plus a command channel for sends/abort and a `listening` AtomicBool. **CRITICAL — match the established pattern:** the existing packet path NEVER runs a blocking KISS link read inside an async task body; `native_packet_connect`'s blocking link I/O is wrapped in `tokio::task::spawn_blocking` (`winlink_backend.rs:1965-1978`; the codebase documents the rule at `winlink_backend.rs:868-869`). A 200 ms-blocking `link.read` plus a sleep on an async executor worker starves the runtime. So the driver is a **plain `fn` (sync)** run via `spawn_blocking`, using `std::sync::mpsc` for commands, `std::thread::sleep` for the nap, and a `std::time::Instant` captured at start for the monotonic `now_ms`.

**msgid minting + capacity live in `AprsState` (synchronous), NOT in the actor.** The actor boundary (the channel) cannot return a capacity verdict to the caller synchronously, and the frontend must know the msgid to render the outgoing bubble. So `AprsState::send` mints the msgid (a monotonic counter), gates on a shared in-flight count, and returns the msgid (or a capacity error) — all synchronously, before the command crosses the channel. The engine's `enqueue_send` receives the already-minted msgid; it never mints.

**Single-host arbitration (from the scope note):** `start` calls `connect_link_with_abort(&cfg, abort)` where `cfg` is `KissLinkConfig::Bluetooth { mac }` (note: the fn takes `&KissLinkConfig` — bind a local, pass `&local`); if it errors (link busy / radio off), return a named error and DO NOT spawn the task. Surface "could not open the radio link — is the packet session using it, or the radio off?".

- [ ] **Step 1: Write the failing test**
```rust
#[test]
fn aprs_state_starts_not_listening() {
    let st = AprsState::default();
    assert!(!st.is_listening());
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement**

Add `use tauri::Emitter;` at the top of engine.rs's Tauri-wiring section (the `AppHandle::emit` method comes from the `Emitter` trait, as in `bootstrap.rs:20`).

Define:
```rust
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::mpsc; // std, NOT tokio — the driver loop is sync (spawn_blocking)

pub enum TxCommand {
    Send { dest: String, text: String, msgid: String },
    Abort,
}

struct AprsHandle {
    cmd_tx: mpsc::Sender<TxCommand>,
    abort: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct AprsState {
    inner: std::sync::Mutex<Option<AprsHandle>>,
    listening: Arc<AtomicBool>,
    counter: AtomicU64,              // monotonic msgid source
    in_flight: Arc<AtomicUsize>,     // shared with the sink; gates capacity
}

impl AprsState {
    pub fn is_listening(&self) -> bool { self.listening.load(Ordering::SeqCst) }

    /// Open the link, build the engine + sink, spawn the blocking driver, store the handle.
    /// Returns a named error if the radio link can't be opened (single-host arbitration).
    pub fn start(&self, app: tauri::AppHandle, mac: String, identity: AprsIdentity) -> Result<(), String> {
        let abort = Arc::new(AtomicBool::new(false));
        let cfg = crate::winlink::ax25::link::KissLinkConfig::Bluetooth { mac };
        let (link, _abort_sock) = crate::winlink::ax25::connect_link_with_abort(&cfg, abort.clone())
            .map_err(|e| format!("could not open the radio link ({e}). Is the packet session using it, or the radio off?"))?;
        let sink: Box<dyn EventSink> = Box::new(TauriEventSink {
            app, in_flight: self.in_flight.clone(),
        });
        let engine = AprsEngine::new(identity, sink);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TxCommand>();
        let listening = self.listening.clone();
        let abort_for_task = abort.clone();
        tokio::task::spawn_blocking(move || run(link, engine, cmd_rx, listening, abort_for_task));
        *self.inner.lock().unwrap() = Some(AprsHandle { cmd_tx, abort });
        Ok(())
    }

    /// Stop listening: flip abort (the driver sees it and exits, emitting listening=false), drop the handle.
    pub fn stop(&self) {
        if let Some(h) = self.inner.lock().unwrap().take() {
            h.abort.store(true, Ordering::SeqCst);
        }
    }

    /// Queue an outgoing message. Mints the msgid, gates on capacity (synchronously, before the
    /// command crosses the channel), increments in-flight, returns the minted msgid.
    pub fn send(&self, dest: String, text: String) -> Result<String, String> {
        let guard = self.inner.lock().unwrap();
        let handle = guard.as_ref().ok_or_else(|| "not listening — start APRS first".to_string())?;
        if self.in_flight.load(Ordering::SeqCst) >= crate::winlink::aprs::tx::CONCURRENT_CAP {
            return Err("too many messages pending — wait for acks or timeouts".into());
        }
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let msgid = mint_msgid(n); // 1–5 alphanumeric, e.g. base-36 of n
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        handle.cmd_tx.send(TxCommand::Send { dest, text, msgid: msgid.clone() })
            .map_err(|_| "APRS driver stopped".to_string())?;
        Ok(msgid)
    }

    /// Flush all pending retransmits (single global abort).
    pub fn abort(&self) {
        if let Some(h) = self.inner.lock().unwrap().as_ref() {
            let _ = h.cmd_tx.send(TxCommand::Abort);
        }
    }
}

/// 1–5 char alphanumeric msgid (base-36 of a monotonic counter, wraps within 5 chars).
fn mint_msgid(n: u64) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut n = n % (36u64.pow(5)); // keep ≤5 chars
    if n == 0 { return "0".into(); }
    let mut s = Vec::new();
    while n > 0 { s.push(ALPHABET[(n % 36) as usize]); n /= 36; }
    s.reverse();
    String::from_utf8(s).unwrap()
}
```
The **sync** driver (run via `spawn_blocking`):
```rust
fn run(
    mut link: Box<dyn crate::winlink::ax25::link::ByteLink>,
    mut engine: AprsEngine,
    cmd_rx: mpsc::Receiver<TxCommand>,
    listening: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
) {
    let started = std::time::Instant::now();
    let now_ms = || started.elapsed().as_millis() as u64; // monotonic
    listening.store(true, Ordering::SeqCst);
    engine.set_listening(true);   // calls self.sink.emit_listening(true)
    let mut buf = [0u8; 1024];
    loop {
        if abort.load(Ordering::SeqCst) { break; }
        // RX: blocking read with the link's built-in 200ms poll timeout.
        match link.read(&mut buf) {
            Ok(0) => break, // genuine EOF / link closed
            Ok(n) => {
                for frame in engine.handle_inbound_bytes(&buf[..n], now_ms()) {
                    let _ = link.write_all(&frame);
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
        // Commands (non-blocking drain)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                TxCommand::Send { dest, text, msgid } => engine.enqueue_send(&dest, &text, &msgid, now_ms()),
                TxCommand::Abort => engine.abort(),
            }
        }
        // Retransmit tick
        for frame in engine.tick(now_ms()) {
            let _ = link.write_all(&frame);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    listening.store(false, Ordering::SeqCst);
    engine.set_listening(false);
}
```
Add to `AprsEngine` (engine.rs, Task 9's type): `pub fn set_listening(&self, on: bool) { self.sink.emit_listening(on); }`.

`TauriEventSink` — decrements the shared in-flight count on TERMINAL states (keeps `AprsState::send`'s capacity gate balanced):
```rust
pub struct TauriEventSink {
    pub app: tauri::AppHandle,
    pub in_flight: Arc<AtomicUsize>,
}
impl EventSink for TauriEventSink {
    fn emit_message(&self, ev: InboundMsg) { let _ = self.app.emit("aprs-message:new", &ev); }
    fn emit_state(&self, ev: StateChange) {
        if ev.state.is_terminal() {
            // saturating decrement
            let _ = self.in_flight.fetch_update(Ordering::SeqCst, Ordering::SeqCst,
                |v| Some(v.saturating_sub(1)));
        }
        let _ = self.app.emit("aprs-message:state", &ev);
    }
    fn emit_listening(&self, on: bool) { let _ = self.app.emit("aprs-listening:change", on); }
}
```
**Serde wire forms (mirror EXACTLY in Task 12):** make `InboundMsg` + `StateChange` derive `serde::Serialize` with `#[serde(rename_all = "camelCase")]`. Make `DeliveryState` derive `serde::Serialize` with `#[serde(rename_all = "camelCase")]` → the four wire strings are **`"sent"`, `"acked"`, `"timedOut"`, `"rejected"`** (NOT `lowercase` — that would emit `"timedout"` and break the TS `'timedOut'` match). Register `.manage(AprsState::default())` in `lib.rs` near the other `.manage(...)` calls (a bare value is fine — Tauri only needs `Send+Sync+'static`; `AprsState`'s `Mutex`/atomics satisfy it. Command signatures then use `State<'_, AprsState>`).

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): async engine driver + AprsState lifecycle`).

---

## Task 11: Tauri commands (`aprs_config_get/set`, `aprs_listen_start/stop`, `aprs_send`, `aprs_abort`)

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add commands + `AprsConfigDto`, mirror `PacketConfigDto` at ~3441–3581)
- Modify: `src-tauri/src/lib.rs` (add all six to `generate_handler![...]` ~569–670)
- Test: `ui_commands.rs` — `AprsConfigDto` ↔ `AprsConfig` round-trip (pure)

**Context:** The command seam. `aprs_listen_start` reads the saved packet BT mac + the `aprs` config, resolves `AprsIdentity` (base call from the active Winlink identity + `source_ssid`; tocall + path from config), and calls `AprsState::start`. `aprs_send` is a thin wrapper over `AprsState::send`, which returns the backend-minted msgid (minting lives entirely in `AprsState`, NOT here). Follow the exact registration pattern: `#[tauri::command]` fn → add to `generate_handler!`.

- [ ] **Step 1: Write the failing test**
```rust
#[test]
fn aprs_config_dto_round_trips() {
    let cfg = crate::config::AprsConfig { source_ssid: 5, tocall: "APZTUX".into(), path: "WIDE2-1".into() };
    let dto = AprsConfigDto::from(&cfg);
    assert_eq!(dto.source_ssid, 5);
    assert_eq!(dto.tocall, "APZTUX");
    let back = dto.into_aprs_config();
    assert_eq!(back, cfg);
}
```

- [ ] **Step 2: (CI verifies)** Expected: FAIL.

- [ ] **Step 3: Implement** the DTO + six commands:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AprsConfigDto {
    pub source_ssid: u8,
    pub tocall: String,
    pub path: String,
}
impl From<&crate::config::AprsConfig> for AprsConfigDto { /* field copy: source_ssid, tocall, path */ }
impl AprsConfigDto { pub fn into_aprs_config(self) -> crate::config::AprsConfig { /* field copy */ } }

#[tauri::command]
pub async fn aprs_config_get() -> Result<AprsConfigDto, UiError> {
    Ok(AprsConfigDto::from(&crate::config::read_config()?.aprs))
}

// NOTE: the JS arg key MUST be `dto` (the frontend invokes `invoke('aprs_config_set', { dto })`).
#[tauri::command]
pub async fn aprs_config_set(state: State<'_, BackendState>, dto: AprsConfigDto) -> Result<(), UiError> {
    // Validate the path before persisting so a bad WIDE string can't brick listen-start.
    crate::winlink::aprs::identity::parse_path(&dto.path).map_err(UiError::from)?;
    let mut cfg = crate::config::read_config()?;
    cfg.aprs = dto.into_aprs_config();
    // ... atomic write via the same path packet_config_set uses ...
    Ok(())
}

#[tauri::command]
pub async fn aprs_listen_start(app: AppHandle, state: State<'_, BackendState>,
    aprs: State<'_, AprsState>) -> Result<(), UiError> {
    let cfg = crate::config::read_config()?;
    // mac from the saved packet link — MUST be a Bluetooth link for 1a (UV-Pro).
    let mac = match &cfg.packet.link {
        Some(crate::winlink::ax25::link::KissLinkConfig::Bluetooth { mac }) => mac.clone(),
        _ => return Err(UiError::from("APRS Phase 1a requires the UV-Pro Bluetooth KISS link; configure it in packet settings first")),
    };
    // Active Winlink base call (the REAL mechanism — ui_commands.rs:3962-3970 is an inline
    // `backend.active_identity()?.mycall()`, NOT a standalone helper):
    let backend = state.current().ok_or_else(|| UiError::from("no active backend"))?;
    let base = backend.active_identity()?.mycall().as_str().to_uppercase();
    let identity = crate::winlink::aprs::identity::AprsIdentity {
        source: crate::winlink::ax25::frame::Address { call: base, ssid: cfg.aprs.source_ssid },
        tocall: crate::winlink::ax25::frame::Address { call: cfg.aprs.tocall.clone(), ssid: 0 },
        path: crate::winlink::aprs::identity::parse_path(&cfg.aprs.path).map_err(UiError::from)?,
    };
    aprs.start(app, mac, identity).map_err(UiError::from)
}

#[tauri::command]
pub async fn aprs_listen_stop(aprs: State<'_, AprsState>) -> Result<(), UiError> { aprs.stop(); Ok(()) }

// Returns the minted msgid so the frontend can render + reconcile the outgoing bubble.
// A CapacityFull / not-listening error propagates as UiError so the UI does NOT show an
// optimistic "sent" bubble for a message that was never queued (RF-honesty).
#[tauri::command]
pub async fn aprs_send(aprs: State<'_, AprsState>, call: String, text: String) -> Result<String, UiError> {
    aprs.send(call, text).map_err(UiError::from)
}

#[tauri::command]
pub async fn aprs_abort(aprs: State<'_, AprsState>) -> Result<(), UiError> { aprs.abort(); Ok(()) }
```
Register all six (`aprs_config_get/set`, `aprs_listen_start/stop`, `aprs_send`, `aprs_abort`) in `lib.rs`'s `generate_handler!` list next to the `packet_*` commands. Verify `UiError: From<String>` exists (it does — used throughout ui_commands.rs); if `active_identity()` returns a different error type, map it with `.map_err(|e| UiError::from(e.to_string()))`. msgid minting lives ENTIRELY in `AprsState::send`/`mint_msgid` (Task 10) — this command does not mint.

- [ ] **Step 4: (CI verifies)** Expected: PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): Tauri commands for APRS chat`).

---

## Task 12: Frontend types + `useAprsChat` event hook

**Files:**
- Create: `src/aprs/aprsTypes.ts`
- Create: `src/aprs/useAprsChat.ts`
- Test: `src/aprs/useAprsChat.test.ts`

**Context:** Mirror the Rust DTO wire shapes EXACTLY (whatever camelCase/tag form Task 10 chose). Subscribe to the three events (`aprs-message:new`, `aprs-message:state`, `aprs-listening:change`) via `listen` from `@tauri-apps/api/event` — the same pattern as `src/connections/useInboundSelection.ts:14-82`. Maintain thread state keyed by callsign; reconcile delivery-state changes by msgid. **Wrap `listen` in a try/catch** (it throws in the vitest/jsdom env — `useInboundSelection` does this).

- [ ] **Step 1: Write the failing test** (`useAprsChat.test.ts`)
```ts
import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

// mock the tauri event module to drive listeners synchronously
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => { delete handlers[name]; });
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

import { useAprsChat } from './useAprsChat';

describe('useAprsChat', () => {
  it('adds an inbound message into the sender thread', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => { /* allow listen() promises to resolve */ });
    act(() => { handlers['aprs-message:new']?.({ payload: { sender: 'KK6XYZ', text: 'ping', msgid: '04' } }); });
    expect(result.current.threads['KK6XYZ']).toBeDefined();
    expect(result.current.threads['KK6XYZ'].messages.at(-1)?.text).toBe('ping');
    expect(result.current.threads['KK6XYZ'].messages.at(-1)?.direction).toBe('in');
  });

  it('inserts an outgoing bubble keyed by the backend-minted msgid and transitions it to acked', async () => {
    // The backend mints the msgid; aprs_send RETURNS it (Task 11). The mock invoke returns 'A1'.
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello'); }); // send(call, text) — NO caller msgid
    // optimistic 'sent' bubble keyed by the RETURNED 'A1'
    expect(result.current.threads['KK6XYZ'].messages.find((x) => x.msgid === 'A1')?.state).toBe('sent');
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    expect(result.current.threads['KK6XYZ'].messages.find((x) => x.msgid === 'A1')?.state).toBe('acked');
  });

  it('does NOT insert a bubble when send is rejected (capacity / not listening)', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('too many messages pending'));
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello').catch(() => {}); });
    // rejected send → no optimistic bubble (RF-honest: never show 'sent' for a never-queued message)
    expect(result.current.threads['KK6XYZ']?.messages ?? []).toHaveLength(0);
  });
});
```
NOTE: the mock `invoke` must resolve to the minted msgid (`'A1'`) for the success case — set `vi.fn().mockResolvedValue('A1')` in the `@tauri-apps/api/core` mock at the top of the file. `send(call, text)` takes NO caller-supplied msgid; it `await`s `invoke<string>('aprs_send', { call, text })` and keys the optimistic bubble on the returned id. On reject it re-throws (or returns) WITHOUT inserting a bubble, and the caller surfaces a toast.

- [ ] **Step 2: Run the test** — `pnpm -C <abs-worktree> vitest run src/aprs/useAprsChat.test.ts`. Expected: FAIL (hook undefined).

- [ ] **Step 3: Implement** `aprsTypes.ts`:
```ts
// MIRROR Task 10's serde wire forms EXACTLY (DeliveryState = camelCase: sent/acked/timedOut/rejected).
export type DeliveryState = 'sent' | 'acked' | 'timedOut' | 'rejected';
export interface InboundMsgDto { sender: string; text: string; msgid: string | null; }
export interface StateChangeDto { msgid: string; state: DeliveryState; }
export interface ChatMessage {
  id: string;            // msgid (or a local uuid for msgid-less inbound)
  direction: 'in' | 'out';
  text: string;
  msgid: string | null;
  state?: DeliveryState; // outgoing only
  at: number;            // Date.now() at insert
}
export interface Thread { callsign: string; messages: ChatMessage[]; }
export interface AprsConfigDto { sourceSsid: number; tocall: string; path: string; }
```
`useAprsChat.ts`: a hook returning `{ threads: Record<string, Thread>, listening: boolean, send, refreshConfig }`. Internally `useState` for threads/listening; `useEffect` to `listen` the three channels (try/catch). `send(call, text)` is **async**: `const id = await invoke<string>('aprs_send', { call, text })`; on success insert an optimistic `out` bubble keyed by `id` with `state: 'sent'`; on reject (capacity/not-listening) **do NOT insert a bubble** — let the error propagate so the panel can toast it. On `aprs-message:new`, append an `in` bubble to `threads[sender]`. On `aprs-message:state`, find by msgid across threads and set `.state` (covers `acked`/`timedOut`/`rejected`). On `aprs-listening:change`, set `listening`.

- [ ] **Step 4: Run the test** — expected PASS. Reap vitest if interrupted.

- [ ] **Step 5: Report DONE** (`feat(aprs): frontend types + useAprsChat event hook`).

---

## Task 13: APRS chat panel (inline surface)

**Files:**
- Create: `src/aprs/AprsChatPanel.tsx`
- Test: `src/aprs/AprsChatPanel.test.tsx`

**Context:** The visible product. A thread list (per callsign) + a conversation view (bubbles, in/out) + a composer (callsign field + text field + Send) + delivery-state chips (sent/ACKed/timed-out) + a listening indicator ("Listening" / "Not listening — radio disconnected"). **Inline only — no pop-up windows** (project rule `feedback_inline_ui_no_window_clutter`). Constrain to a realistic reading-pane width, do not stretch full-width (`feedback_no_stretched_full_width_ui`). Use Ctrl-first modifier hints if any. Honest states only — NO fake "delivered" check (`RF-honest UX`). Drive everything off `useAprsChat`.

- [ ] **Step 1: Write the failing test** (render-the-production-path)
```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));
import { AprsChatPanel } from './AprsChatPanel';

describe('AprsChatPanel', () => {
  it('renders the composer and a listening indicator', () => {
    render(<AprsChatPanel />);
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('shows the empty-state guidance when no threads', () => {
    render(<AprsChatPanel />);
    expect(screen.getByText(/no conversations yet/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test** — expected FAIL.

- [ ] **Step 3: Implement** the panel. Structure: a flex container (sidebar thread list + main pane), composer at the bottom of the main pane. Delivery chips for ALL four states: `sent` = "Sent" (neutral), `acked` = "Acked" (success), `timedOut` = "Timed out" (warning), `rejected` = "Rejected" (error). Listening indicator with `data-testid="aprs-listening-indicator"` reflecting `listening`. Empty state "No conversations yet — send a message or wait for inbound." Wire Send to `useAprsChat().send` (async; on a rejected send, surface the error message as an inline notice — do NOT insert a bubble). Match existing panel styling conventions (look at `src/radio/modes/PacketRadioPanel.tsx` for class names / structure). Keep it constrained-width. The Start/Stop listening toggle is added in Task 14 (don't add it here — Task 14 owns it).

- [ ] **Step 4: Run the test + tsc** — expected PASS + clean typecheck.

- [ ] **Step 5: Report DONE** (`feat(aprs): inline APRS chat panel`).

---

## Task 14: Mount the panel + APRS identity settings

**Files:**
- Modify: `src/shell/AppShell.tsx` (lazy-mount `AprsChatPanel` inline, mirroring the packet lazy entry `loadPacketRadioPanel = () => import('../radio/modes/PacketRadioPanel').then((m) => ({ default: m.PacketRadioPanel }))` at AppShell.tsx ~143-152 — add the analogous `loadAprsChatPanel` + `lazy(...)` + a nav/mode entry)
- Modify: `src/aprs/AprsChatPanel.tsx` (Task 13's panel) — ADD the Start/Stop listening toggle here (Task 14 owns it)
- Create: `src/aprs/AprsSettings.tsx` (CALL-SSID / path / tocall, persisted via `aprs_config_set`)
- Modify: wherever `SettingsPanel` composes its sections — add the APRS settings section
- Test: `src/aprs/AprsSettings.test.tsx` (+ extend `AprsChatPanel.test.tsx` for the toggle)

**Context:** Make the panel reachable and the identity configurable. The chat panel mounts inline in the existing shell navigation (NO new window). APRS settings let the operator set the source SSID and confirm/edit the path; tocall defaults to `APZTUX` and is shown read-only for 1a (reduces footguns). **The Start/Stop listening toggle is owned by THIS task** (an explicit toggle, NOT auto-start-on-mount — auto-start would fight the single-host arbitration). Add it to `AprsChatPanel`, wired to `aprs_listen_start`/`aprs_listen_stop`, and add a test asserting a Start/Stop control renders. Do NOT rely on Task 13 having added it (Task 13 explicitly defers it here).

- [ ] **Step 1: Write the failing test** (`AprsSettings.test.tsx`)
```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
const invoke = vi.fn().mockResolvedValue({ sourceSsid: 0, tocall: 'APZTUX', path: 'WIDE1-1,WIDE2-1' });
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import { AprsSettings } from './AprsSettings';

describe('AprsSettings', () => {
  it('loads and displays the current APRS config', async () => {
    render(<AprsSettings />);
    await waitFor(() => expect(screen.getByDisplayValue('WIDE1-1,WIDE2-1')).toBeInTheDocument());
    expect(screen.getByText('APZTUX')).toBeInTheDocument();
  });

  it('persists a changed path via aprs_config_set', async () => {
    render(<AprsSettings />);
    await waitFor(() => screen.getByDisplayValue('WIDE1-1,WIDE2-1'));
    fireEvent.change(screen.getByLabelText(/path/i), { target: { value: 'WIDE2-1' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('aprs_config_set',
      expect.objectContaining({ dto: expect.objectContaining({ path: 'WIDE2-1' }) })));
  });
});
```

- [ ] **Step 2: Run the test** — expected FAIL.

- [ ] **Step 3: Implement** `AprsSettings.tsx` (load via `aprs_config_get`, save via `aprs_config_set` — invoke arg key MUST be `dto`: `invoke('aprs_config_set', { dto })`; read returns camelCase `AprsConfigDto` with `sourceSsid`); add the lazy mount in `AppShell.tsx` (mirror `loadPacketRadioPanel`/`lazy(...)` + the nav/mode entry); add the settings section to `SettingsPanel`. Add the Start/Stop listening toggle to `AprsChatPanel` (wired to `aprs_listen_start`/`aprs_listen_stop`) — this task owns it; add a test that the toggle renders.

- [ ] **Step 4: Run the test + tsc + a broad scoped vitest** (`pnpm -C <abs-worktree> vitest run src/aprs`) — expected PASS.

- [ ] **Step 5: Report DONE** (`feat(aprs): mount chat panel inline + APRS identity settings`).

---

## Final integration review (orchestrator, after Task 14)

1. **Dispatch the final code reviewer** over the whole `winlink/aprs/` module + the frontend `src/aprs/` + the touched files, per subagent-driven-development's final-review step.
2. **Self-adrev** (Codex substitute per the standing operator decision until Codex quota returns): two adversarial passes over the full diff, grounded against the real code. Attack angles: the promiscuous RX loop (does it actually bypass the dest filter?); the **dedupe-vs-ack-throttle split** (does a sender's retransmit still get re-ACKed — lost-ACK recovery — while a digipeated burst collapses to one ACK?); the retransmit arithmetic under **irregular ticks** (does the final retry keep its full ACK window?); the ack addressee direction (auto-ack addressed to the SENDER, echoes their msgid, lowercase); the **in-flight counter balance** (does `send` +1 and every terminal state −1 keep the capacity gate honest, or can it leak/underflow?); the `spawn_blocking` driver (no blocking read on the async executor); the single-host arbitration error path; and the msgid round-trip frontend↔backend (no optimistic bubble on a rejected send).
3. **Confirm CI green** on the draft PR (`gh pr checks <n>`) — both arches, clippy --all-targets, full vitest. Re-run until exit 0 (clippy hides later-target lints).
4. **Update the spec** with a one-line "Phase 1a built" note + the `APZTUX` tocall decision.
5. **bd**: keep `tuxlink-2f2n` in_progress (Phase 1a is a slice); file `bd create` follow-ups for 1b (multi-transport, channel-monitor + per-SSID/category filtering, listening-state polish, the `rej`-handling UX, cross-arbitration with the packet session).
6. **Operator on-air smoke (RADIO-1, operator-only):** UV-Pro BT KISS on a real APRS frequency — send to a known station / a second radio, confirm inbound threads + ACK round-trip + that Stop/abort de-keys cleanly. Agent never transmits.

---

## Self-review against the spec (writing-plans checklist) + plan-review round 1 dispositions

**Spec coverage:** send/recv text over UV-Pro BT KISS → Tasks 7–11 (TX/RX) + 13 (UI). States sent→ACKed→timed-out (+rejected) → Task 8 (state machine) + 9 (emit) + 12/13 (display). Default path WIDE1-1,WIDE2-1 → Task 5. Mandatory dedupe → Task 6 + 9. Net-new promiscuous RX listener → Task 9/10. `Control::Ui` net-new codec → Task 1. APRS identity separate from Winlink → Task 5 + 11. Exact message format pinned → Tasks 2-4 (the grounded vectors). RADIO-1 retry/serial-queue/cap/abort → Task 8 + RADIO-1 section. Threads view → Task 13 (channel-monitor + per-SSID filtering correctly deferred to 1b). Listening indicator → Task 9/13/14.

**Plan-review round 1 (3 parallel adversarial passes, self-adrev) — dispositions, all FIXED in this plan:**
- BLOCKER (codebase): `Config` has no `Default` + exhaustive literal sites → **Task 5** now greps + patches every `Config { … }` site; JSON-not-TOML reframed.
- BLOCKER (codebase): blocking read in async → **Task 10** now uses `tokio::task::spawn_blocking` + a sync `run` (std mpsc, std sleep, `Instant` clock), matching `native_packet_connect`.
- BLOCKER (subagent): `aprs_send` return type + msgid lifecycle → **Task 11** `aprs_send -> Result<String, UiError>`; minting lives ONLY in `AprsState::send` (Task 10); engine `enqueue_send` receives a pre-minted id.
- BLOCKER (subagent): `DeliveryState` `lowercase` can't emit `timedOut` → **Task 10** pins `camelCase` (`sent`/`acked`/`timedOut`/`rejected`); Task 12 mirrors exactly.
- BLOCKER (APRS): dedupe suppressed the auto-ACK → **Task 9** splits a long display-dedupe window from a short `ack_throttle` window so lost-ACK recovery works without an ACK storm; new re-ACK test.
- BLOCKER (APRS/RF-honest): capacity-rejected send → stuck "sent" → **Tasks 9/10/11/12** propagate `CapacityFull` as `UiError`; frontend inserts the optimistic bubble ONLY on success; new reject test.
- CONCERN (APRS): tick jitter could compress the final ACK window / skip a retransmit → **Task 8** anchors timeout on grace-since-last-send + one-send-per-tick; new irregular-tick tests.
- CONCERN (APRS): REJ rode the 150s timeout → **Task 9** stops retransmit on REJ + terminal `Rejected` state.
- CONCERN (codebase): identity-resolution "helper" was a comment → **Task 11** uses the real `backend.active_identity()?.mycall()`; `connect_link_with_abort(&cfg, …)` by-ref; `use tauri::Emitter;` noted.
- CONCERN (subagent): toggle ownership + `dto` arg name + `pub mod`/`mod tests` discipline → **Tasks 13/14** assign the toggle to Task 14; `dto` pinned; the Cross-cutting conventions block + per-task notes on Tasks 3/4.

**Type consistency:** `AprsPayload`, `DeliveryState` (Rust `Sent/Acked/TimedOut/Rejected` ↔ wire `sent/acked/timedOut/rejected`), `InboundMsg`/`StateChange`, `AprsIdentity`, `DedupeKey`, `TxQueue`/`DueSend`/`TxError`, `AprsConfig`/`AprsConfigDto`, `TxCommand`, `AprsState`/`AprsEngine`/`TauriEventSink` are named identically across tasks; the TS mirrors in Task 12 match the camelCase wire forms.

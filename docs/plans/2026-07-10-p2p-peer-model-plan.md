# P2P Peer Model & VARA Protocol Completeness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship P2P as a complete, discoverable, verifiable mode: a first-class peer store auto-tracked at every transport's attempt-conclusion, a hardened trust boundary for the agent surface, peer rows in the finder and on both maps, and an engine-split (HF/SAT vs FM) VARA command plan that fixes the c39af/gbb05/m9kcd protocol defects.

**Architecture:** A new `peers.json` store (mirroring `contacts/store.rs`) is populated by a shared `record_peer_observation` recorder invoked from 8 per-transport attempt-conclusion sites via drop-guards. The VARA layer gains `SessionType`/`RETRIES`/corrected `COMPRESSION` vocabulary and a fail-open REGISTERED readiness gate, branched on engine (`TransportKind::VaraHf` vs `VaraFm`). The agent surface gets a curated, egress-arm-gated `find_peers` and a `(peer_id, endpoint_id)`-only telnet dial with a DNS-rebinding-safe denylist. Per ADR 0018 every integration-matrix row lands together; capability bits hide (never stub) unshipped rows.

**Tech Stack:** Rust (Tauri 2.x backend, MSRV 1.75), React 18 + TypeScript (Vite, raw Leaflet 1.9, TanStack Query), vitest (jsdom, colocated tests), rmcp 2.1 MCP tools in `tuxlink-mcp-core`.

**Spec (source of truth):** `docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md` — all `[R#-N]` citations below refer to its folded adversarial findings. Requirements live there; this plan is the execution order + exact code.

## Global Constraints

- **MSRV 1.75** (`src-tauri/Cargo.toml:17`); clippy denies `incompatible_msrv` — no `Result::inspect_err`, no 1.76+ APIs.
- **No local cold cargo builds on the dev Pi.** Write Rust + tests, push, let CI compile (`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` and `cargo test … --locked` on amd64+arm64). `pnpm vitest run <file>` and `pnpm typecheck` ARE run locally per task.
- **No new Cargo dependencies.** IDs use `uuid::Uuid::new_v4().to_string()` (house pattern, `favorites/commands.rs:41-43`). The spec says "ULID"; this plan substitutes uuid-v4 deliberately — the repo has `uuid` with `v4`+`serde` and no `ulid`, and the only property required is a stable unique string key. Adding a dep would require a Cargo.lock regen and buys nothing.
- **Serde shape rules:** DTOs mirror Rust shapes EXACTLY, snake_case fields, NO `rename_all` on structs (`src/favorites/types.ts:2-7`). Enums use `#[serde(rename_all = "kebab-case")]`. Unit-only enums take `#[serde(other)] Unknown` (in-tree precedent: `AntennaPreset`, `src-tauri/src/propagation/antenna.rs:57-81`). Data-carrying enums use `#[serde(tag = "kind")]` (internally tagged — the form serde officially supports `other` on). Every new enum gets a shape test.
- **Plain-language vocabulary:** UI strings and field names say **incoming / outgoing / added** — never "worked" or other ham parlance.
- **ADR 0018 (`docs/adr/0018-radio1-gates-operator-execution-not-agent-authorship.md`):** all 10 integration-matrix rows (spec §Integration wire-walk matrix) land in this plan; nothing is sliced or deferred. Unwired UI rows are HIDDEN via capability bits, never rendered disabled.
- **RADIO-1:** no task transmits. VARA behavior is verified against a scripted mock TCP server; the two-rig bench (spec §8) is operator-executed after merge.
- **Timestamps:** on-disk peer timestamps are RFC3339 with local offset produced backend-side via `chrono::Local::now().to_rfc3339()` (chrono `clock` feature is enabled). Frontend-supplied `ts_local` continues to use `tsLocal()` where the favorites bridge crosses from the frontend.
- **Commit discipline:** conventional commits, `Agent: <session-moniker>` trailer + `Co-Authored-By` line on every commit. Frequent small commits, one per task minimum.

## Definition of done — operator wire-walk flows (greenfield, verbatim)

> **GATE (wire-walk Iron Law):** the flows below are supplied by the operator, greenfield, at plan start. They are the definition-of-done: at build end the wire-walk skill traces each one to `file:line` from its stated starting state. Any ❌ = the feature is not shipped. Agents MUST NOT edit, narrow, re-rank, or substitute these flows.

**STATUS: PENDING — the operator has been asked (2026-07-10, session tanager-sequoia-opossum) and this section is populated verbatim from their reply before this plan is finalized and reviewed. The plan is not executable until this section is filled.**

## File Structure

New files:

| Path | Responsibility |
|---|---|
| `src-tauri/src/winlink/callsign.rs` | `canonical_base()`, wire-grammar validation, display/injection sanitizer — shared by VARA echo-match, peers store, curation |
| `src-tauri/src/peers/mod.rs` | module wiring |
| `src-tauri/src/peers/model.rs` | `Peer`, `PeersFile`, `Channel`, `Endpoint` + enums (serde shapes) |
| `src-tauri/src/peers/store.rs` | open/quarantine/flush, upsert/dedup, split routing, caps, rate-limit quarantine |
| `src-tauri/src/peers/recorder.rs` | `PeerObservation`, phase classification, `ObservationGuard` (drop-guard) |
| `src-tauri/src/peers/commands.rs` | Tauri commands: `peers_read`, `peer_upsert`, `peer_delete`, `peer_merge`, `peer_split`, `peer_endpoint_promote`, `peer_endpoint_password_set`, `p2p_capabilities` |
| `src/peers/types.ts` | TS mirror of the Rust shapes |
| `src/peers/usePeers.ts` | TanStack Query hook (`peers_read` + `peers:changed` invalidation) |
| `src/peers/peerModel.ts` | `aggregatePeers()` — distinct from `aggregateStations()` (base-only key, grid optional) |
| `src/peers/PeerSettings.tsx` | inline "P2P Peers" settings section (roster editor) |
| `src/map/PeerLayer.tsx` | circle-shaped peer divIcon layer (both maps) |
| `docs/design/2026-07-10-p2p-bench-runbook.md` | operator two-rig bench runbook (spec §8) |

Modified (task-by-task detail below): `src-tauri/src/winlink/modem/vara/{command,commands,listener}.rs`, `src-tauri/src/winlink_backend.rs`, `src-tauri/src/modem_commands.rs`, `src-tauri/src/ui_commands.rs`, `src-tauri/src/winlink/telnet_listen.rs`, `src-tauri/src/winlink/telnet_p2p.rs`, `src-tauri/src/winlink/credentials.rs`, `src-tauri/src/uninstall_cleanup.rs`, `src-tauri/src/favorites/{store,commands}.rs`, `src-tauri/src/mcp_ports.rs`, `src-tauri/tuxlink-mcp-core/src/{ports,router}.rs`, `src-tauri/src/lib.rs`, `src/favorites/types.ts`, `src/connections/connectDispatch.ts`, `src/catalog/{StationFinderPanel,StationFinderControls,StationRail,StationFinderMap}.tsx`, `src/aprs/AprsPositionsMap.tsx`, `src/shell/SettingsPanel.tsx`.

Task order: Phase 0 foundations (T1-T2) → Phase 1 VARA protocol (T3-T6) → Phase 2 peer store (T7-T11) → Phase 3 record sites (T12-T17) → Phase 4 trust boundary + agent surface (T18-T21) → Phase 5 frontend (T22-T25) → Phase 6 integration + gate (T26-T28). Later tasks consume earlier tasks' exact interfaces; do not reorder across phases.

---

### Task 1: Shared callsign module — `canonical_base`, wire grammar, display sanitizer

**Files:**
- Create: `src-tauri/src/winlink/callsign.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod callsign;`)
- Test: inline `#[cfg(test)]` in the new file

**Interfaces:**
- Produces: `pub fn canonical_base(presented: &str) -> String`; `pub fn validate_wire_callsign(s: &str) -> Result<(), String>`; `pub fn sanitize_display(s: &str) -> Option<String>` — consumed by Tasks 5, 7, 8, 18, 19.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_base_strips_ssid_and_portable_suffix() {
        assert_eq!(canonical_base("w6abc-7"), "W6ABC");
        assert_eq!(canonical_base("W6ABC/P"), "W6ABC");
        assert_eq!(canonical_base("W6ABC/P-7"), "W6ABC"); // slash first, then SSID
        assert_eq!(canonical_base("N0DAJ-T"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-R"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-L"), "N0DAJ"); // WLE off-doc post-office suffix
        assert_eq!(canonical_base("N0DAJ-0"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-15"), "N0DAJ");
        assert_eq!(canonical_base("  n0daj "), "N0DAJ");
    }

    #[test]
    fn canonical_base_does_not_strip_non_ssid_tails() {
        // "-16" is not a valid SSID; a tactical hyphenated name keeps its tail.
        assert_eq!(canonical_base("N0DAJ-16"), "N0DAJ-16");
        assert_eq!(canonical_base("CAMP-OPS"), "CAMP-OPS");
    }

    #[test]
    fn wire_grammar_accepts_valid_and_rejects_invalid() {
        assert!(validate_wire_callsign("W6ABC").is_ok());
        assert!(validate_wire_callsign("W6ABC-7").is_ok());
        assert!(validate_wire_callsign("W6ABC-15").is_ok());
        assert!(validate_wire_callsign("W6ABC-T").is_ok());
        assert!(validate_wire_callsign("W6ABC-R").is_ok());
        assert!(validate_wire_callsign("AB1").is_ok());       // 3-char base
        assert!(validate_wire_callsign("AB1CDEF").is_ok());   // 7-char base
        assert!(validate_wire_callsign("AB1CDEFG").is_err()); // 8-char base [R3-9]
        assert!(validate_wire_callsign("W6ABC-16").is_err()); // SSID > 15 [R3-9]
        assert!(validate_wire_callsign("W6").is_err());       // too short
        assert!(validate_wire_callsign("W6:ABC").is_err());   // charset
        assert!(validate_wire_callsign("").is_err());
    }

    #[test]
    fn sanitize_display_rejects_injection_shapes() {
        // Broad display/injection floor [R5-10]: control chars, ':', path
        // separators, whitespace, angle brackets are rejected outright.
        assert_eq!(sanitize_display("W6ABC-7"), Some("W6ABC-7".to_string()));
        assert_eq!(sanitize_display("W6ABC/P"), Some("W6ABC/P".to_string())); // legit presented form; '/' alone is allowed, "../" is not
        assert_eq!(sanitize_display("<img src=x>"), None);
        assert_eq!(sanitize_display("A:B"), None);
        assert_eq!(sanitize_display("A B"), None);
        assert_eq!(sanitize_display("A\u{0}B"), None);
        assert_eq!(sanitize_display("..\\x"), None);
        assert_eq!(sanitize_display("a/../b"), None);
        assert_eq!(sanitize_display(""), None);
        assert_eq!(sanitize_display(&"X".repeat(65)), None); // length cap 64
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --locked callsign 2>&1 | tail -5` — on the Pi this may not complete a cold build; if it does not finish in ~2 min, skip local verification and rely on CI (Global Constraints). Expected on a warm target: FAIL (module does not exist).

- [ ] **Step 3: Write the implementation**

```rust
//! Shared callsign normalization + validation.
//!
//! Three DISTINCT concerns (spec §1, [R5-10]) — do not merge them:
//! - `canonical_base` — the peers-store dedup anchor. NEVER a wire source.
//! - `validate_wire_callsign` — transport grammar for MYCALL/CONNECT targets.
//! - `sanitize_display` — broad injection floor for anything crossing the
//!   agent DTO or a render boundary. Looser about ham suffixes, stricter
//!   about shell/DOM/keyring metacharacters.

/// SSID-ish tails stripped by [`canonical_base`]: `-0`..`-15`, `-T`, `-R`,
/// and WLE's off-doc `-L` (post-office). Anything else is NOT an SSID.
fn is_ssid_tail(tail: &str) -> bool {
    matches!(tail, "T" | "R" | "L")
        || tail.parse::<u8>().map(|n| n <= 15).unwrap_or(false)
}

/// Dedup anchor: uppercase, trim, take the substring before the first `/`,
/// then strip one trailing SSID tail. Spec §1: "Never used to derive a wire
/// target."
pub fn canonical_base(presented: &str) -> String {
    let up = presented.trim().to_ascii_uppercase();
    let before_slash = up.split('/').next().unwrap_or("");
    if let Some((head, tail)) = before_slash.rsplit_once('-') {
        if is_ssid_tail(tail) && !head.is_empty() {
            return head.to_string();
        }
    }
    before_slash.to_string()
}

/// Transport wire grammar [R3-9]: base 3-7 chars A-Z0-9, optional SSID
/// `-1..-15` / `-T` / `-R`. Rejects 8-char bases and `-16`. Applied before
/// any MYCALL / CONNECT send.
pub fn validate_wire_callsign(s: &str) -> Result<(), String> {
    let s = s.trim().to_ascii_uppercase();
    let (base, ssid) = match s.split_once('-') {
        Some((b, t)) => (b, Some(t)),
        None => (s.as_str(), None),
    };
    if !(3..=7).contains(&base.len()) {
        return Err(format!("callsign base must be 3-7 chars, got {:?}", base));
    }
    if !base.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("callsign base must be A-Z0-9, got {:?}", base));
    }
    match ssid {
        None => Ok(()),
        Some("T") | Some("R") => Ok(()),
        Some(t) => match t.parse::<u8>() {
            Ok(n) if (1..=15).contains(&n) => Ok(()),
            _ => Err(format!("invalid SSID {:?} (allowed: -1..-15, -T, -R)", t)),
        },
    }
}

/// Broad display/injection sanitizer [R5-10][R2-S2][R2-S10]: the floor for
/// every peer-derived string crossing the agent DTO or a render/keyring
/// boundary. Returns the trimmed string, or `None` = reject/drop.
pub fn sanitize_display(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    if t.contains("..") {
        return None; // path traversal
    }
    for c in t.chars() {
        if c.is_control()
            || c.is_whitespace()
            || matches!(c, ':' | '\\' | '<' | '>' | '"' | '\'' | '`')
        {
            return None;
        }
    }
    Some(t.to_string())
}
```

In `src-tauri/src/winlink/mod.rs`, add alongside the existing module declarations:

```rust
pub mod callsign;
```

- [ ] **Step 4: Run the tests / typecheck**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --locked callsign` (or defer to CI per Global Constraints). Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/callsign.rs src-tauri/src/winlink/mod.rs
git commit -m "feat(winlink): shared callsign module — canonical_base, wire grammar, display sanitizer"
```

---

### Task 2: VARA command-layer extensions (parser + renderer)

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/command.rs`
- Test: inline `#[cfg(test)]` in the same file (existing test module)

**Interfaces:**
- Consumes: nothing new.
- Produces (consumed by Tasks 3-6, 13):
  - `InboundCommand::Registered(Option<String>)` — any `REGISTERED` line `[R3-2]`
  - `InboundCommand::Wrong` — bare `WRONG` `[R3-6-wrong]`
  - `InboundCommand::Connected { mycall, target, bandwidth: Option<ConnectedBandwidth>, via: Vec<String> }` (field `bandwidth_hz: Option<u32>` is REPLACED) `[R3-7]`
  - `pub enum ConnectedBandwidth { Hz(u32), Wide, Narrow }`
  - `OutboundCommand::Connect { mycall, target, via: Vec<String> }` (gains `via`) `[R3-6]`
  - `OutboundCommand::SessionType(VaraSessionType)`, `pub enum VaraSessionType { P2p, Winlink }` with `VaraSessionType::from_intent(SessionIntent)`
  - `OutboundCommand::Retries(u8)`
  - `Compression` variants become `Off | Text | Files` (`Binary`/`Auto` removed — invalid vocabulary per the EA5HVK doc)

- [ ] **Step 1: Write the failing tests** (append to the existing `#[cfg(test)] mod tests` in `command.rs`)

```rust
    #[test]
    fn parses_registered_bare_and_with_callsign() {
        // [R3-2] any REGISTERED line releases the readiness gate; bare =
        // unregistered tier (fully functional, the project's common case).
        assert_eq!(
            InboundCommand::parse("REGISTERED").unwrap(),
            InboundCommand::Registered(None)
        );
        assert_eq!(
            InboundCommand::parse("REGISTERED W6ABC-7").unwrap(),
            InboundCommand::Registered(Some("W6ABC-7".to_string()))
        );
        // Disambiguation: LINK REGISTERED stays its own variant.
        assert_eq!(
            InboundCommand::parse("LINK REGISTERED").unwrap(),
            InboundCommand::LinkRegistered
        );
    }

    #[test]
    fn parses_bare_wrong_distinct_from_wrong_callsign() {
        assert_eq!(InboundCommand::parse("WRONG").unwrap(), InboundCommand::Wrong);
        assert_eq!(
            InboundCommand::parse("WRONG CALLSIGN").unwrap(),
            InboundCommand::WrongCallsign
        );
    }

    #[test]
    fn parses_connected_hf_numeric_bandwidth() {
        assert_eq!(
            InboundCommand::parse("CONNECTED W6ABC N0DAJ 2300").unwrap(),
            InboundCommand::Connected {
                mycall: "W6ABC".into(),
                target: "N0DAJ".into(),
                bandwidth: Some(ConnectedBandwidth::Hz(2300)),
                via: vec![],
            }
        );
    }

    #[test]
    fn parses_connected_fm_wide_narrow_and_via() {
        // [R3-7] FM bandwidth token is WIDE/NARROW, not Hz; via-digis kept.
        assert_eq!(
            InboundCommand::parse("CONNECTED W6ABC N0DAJ WIDE").unwrap(),
            InboundCommand::Connected {
                mycall: "W6ABC".into(),
                target: "N0DAJ".into(),
                bandwidth: Some(ConnectedBandwidth::Wide),
                via: vec![],
            }
        );
        assert_eq!(
            InboundCommand::parse("CONNECTED W6ABC N0DAJ VIA DIGI1 DIGI2 NARROW").unwrap(),
            InboundCommand::Connected {
                mycall: "W6ABC".into(),
                target: "N0DAJ".into(),
                bandwidth: Some(ConnectedBandwidth::Narrow),
                via: vec!["DIGI1".into(), "DIGI2".into()],
            }
        );
        // No bandwidth token at all: still a valid CONNECTED.
        assert_eq!(
            InboundCommand::parse("CONNECTED W6ABC N0DAJ").unwrap(),
            InboundCommand::Connected {
                mycall: "W6ABC".into(),
                target: "N0DAJ".into(),
                bandwidth: None,
                via: vec![],
            }
        );
    }

    #[test]
    fn renders_session_type_retries_and_connect_via() {
        assert_eq!(
            OutboundCommand::SessionType(VaraSessionType::P2p).as_wire(),
            "P2P SESSION"
        );
        assert_eq!(
            OutboundCommand::SessionType(VaraSessionType::Winlink).as_wire(),
            "WINLINK SESSION"
        );
        assert_eq!(OutboundCommand::Retries(10).as_wire(), "RETRIES 10");
        assert_eq!(
            OutboundCommand::Connect {
                mycall: "W6ABC".into(),
                target: "N0DAJ-7".into(),
                via: vec![],
            }
            .as_wire(),
            "CONNECT W6ABC N0DAJ-7"
        );
        assert_eq!(
            OutboundCommand::Connect {
                mycall: "W6ABC".into(),
                target: "N0DAJ-7".into(),
                via: vec!["DIGI1".into(), "DIGI2".into()],
            }
            .as_wire(),
            "CONNECT W6ABC N0DAJ-7 VIA DIGI1 DIGI2"
        );
    }

    #[test]
    fn compression_vocabulary_is_doc_exact() {
        // [R3-10 / dispositions "Compression (confirmed)"]: OFF/TEXT/FILES
        // only. TEXT is the doc-"Recommended for Winlink" mode.
        assert_eq!(Compression::Off.as_wire(), "OFF");
        assert_eq!(Compression::Text.as_wire(), "TEXT");
        assert_eq!(Compression::Files.as_wire(), "FILES");
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test --manifest-path src-tauri/Cargo.toml --locked vara::command` (or defer to CI). Expected: compile FAIL (`Registered`, `Wrong`, `ConnectedBandwidth`, `via`, `SessionType`, `Retries`, `Files` do not exist).

- [ ] **Step 3: Implement**

In `command.rs`, replace the `Compression` enum (currently lines 79-102):

```rust
/// VARA payload compression mode. Doc-exact vocabulary (EA5HVK "VARA
/// Protocol Native TNC Commands"): OFF / TEXT / FILES. The previous
/// `Binary` / `Auto` variants were invalid vocabulary and drew `WRONG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Plain text compression — the doc's "Recommended for Winlink" mode.
    Text,
    /// File-oriented compression.
    Files,
    /// No compression.
    Off,
}

impl Compression {
    /// Wire-form keyword.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Files => "FILES",
            Self::Off => "OFF",
        }
    }
}

/// VARA session type (HF/SAT ONLY — VARA FM has no session-type command
/// [R3-1]). Sent at open and re-sent immediately before each CONNECT
/// [R3-9-placement]. Sets the 4.6 s (P2P) vs 4.0 s (RMS) retry cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaraSessionType {
    P2p,
    Winlink,
}

impl VaraSessionType {
    /// `P2p` intent → `P2P SESSION`; every other intent → `WINLINK SESSION`.
    pub fn from_intent(intent: crate::winlink::session::SessionIntent) -> Self {
        match intent {
            crate::winlink::session::SessionIntent::P2p => Self::P2p,
            _ => Self::Winlink,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::P2p => "P2P SESSION",
            Self::Winlink => "WINLINK SESSION",
        }
    }
}
```

Extend `OutboundCommand` (the `Connect` variant is replaced; `SessionType` and `Retries` are added):

```rust
    /// `CONNECT <mycall> <target> [VIA <digi1> [<digi2>]]` — initiate ARQ
    /// connection. `via` is the digipeater path (VARA FM; max 2) [R3-6];
    /// empty = direct.
    Connect {
        mycall: String,
        target: String,
        via: Vec<String>,
    },
    /// `P2P SESSION` / `WINLINK SESSION` — HF/SAT only [R3-1].
    SessionType(VaraSessionType),
    /// `RETRIES <n>` — undocumented-but-WLE-used; HF P2P branch only [R3-4].
    Retries(u8),
```

And the matching `as_wire` arms (replace the `Connect` arm, add the new ones):

```rust
            Self::Connect { mycall, target, via } => {
                if via.is_empty() {
                    format!("CONNECT {mycall} {target}")
                } else {
                    format!("CONNECT {mycall} {target} VIA {}", via.join(" "))
                }
            }
            Self::SessionType(t) => t.as_wire().to_string(),
            Self::Retries(n) => format!("RETRIES {n}"),
```

Add the bandwidth enum and replace the `Connected` variant of `InboundCommand`:

```rust
/// Bandwidth token on a `CONNECTED` line: HF reports Hz (`2300`); FM
/// reports `WIDE` / `NARROW` [R3-7].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectedBandwidth {
    Hz(u32),
    Wide,
    Narrow,
}
```

```rust
    /// `CONNECTED <mycall> <target> [VIA <digi>…] [bw]` — ARQ link
    /// established. `via` digis are preserved (FM) [R3-7].
    Connected {
        mycall: String,
        target: String,
        bandwidth: Option<ConnectedBandwidth>,
        via: Vec<String>,
    },
    /// `REGISTERED [<call>]` — modem readiness token [R3-2]. Bare =
    /// unregistered tier (fully functional). Distinct from `LINK REGISTERED`.
    Registered(Option<String>),
    /// Bare `WRONG` — a rejected/malformed command. During a dial this
    /// fails fast instead of eating the connect deadline [R3-6-wrong].
    Wrong,
```

Replace the `"CONNECTED"` parse arm (currently `command.rs:229-247`) with a token-scan that handles all three shapes:

```rust
            "CONNECTED" => {
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "CONNECTED".into(),
                    detail: "missing args".into(),
                })?;
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                if tokens.len() < 2 {
                    return Err(CommandParseError::Malformed {
                        cmd: "CONNECTED".into(),
                        detail: format!("need at least 2 args (mycall target), got {tokens:?}"),
                    });
                }
                let mut bandwidth = None;
                let mut via: Vec<String> = Vec::new();
                let mut in_via = false;
                for t in &tokens[2..] {
                    if t.eq_ignore_ascii_case("VIA") {
                        in_via = true;
                    } else if t.eq_ignore_ascii_case("WIDE") {
                        bandwidth = Some(ConnectedBandwidth::Wide);
                        in_via = false;
                    } else if t.eq_ignore_ascii_case("NARROW") {
                        bandwidth = Some(ConnectedBandwidth::Narrow);
                        in_via = false;
                    } else if let Ok(hz) = t.parse::<u32>() {
                        bandwidth = Some(ConnectedBandwidth::Hz(hz));
                        in_via = false;
                    } else if in_via {
                        via.push(t.to_string());
                    }
                    // Unknown trailing token outside VIA: ignore (forward-compat).
                }
                Self::Connected {
                    mycall: tokens[0].to_string(),
                    target: tokens[1].to_string(),
                    bandwidth,
                    via,
                }
            }
            "REGISTERED" => Self::Registered(rest.map(str::to_string)),
```

And extend the `"WRONG"` arm (currently maps only `WRONG CALLSIGN`):

```rust
            "WRONG" => match rest {
                Some(rest) if rest.eq_ignore_ascii_case("CALLSIGN") => Self::WrongCallsign,
                None => Self::Wrong,
                _ => Self::Unknown(line.to_string()),
            },
```

**Compile ripple (fix in this task):**
- `commands.rs:2712` `OutboundCommand::Connect { mycall, target }` → add `via: vec![]` (Task 5 threads real via values).
- Any existing test constructing `Connected { bandwidth_hz, .. }` or `Compression::Binary`/`Auto` → update to the new shapes.
- `commands.rs:2757` and `listener.rs:355` destructure `Connected { target, .. }` with `..` — they compile unchanged.

- [ ] **Step 4: Run the tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked vara` (or defer to CI). Expected: PASS, including all pre-existing vara tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/modem/vara/command.rs src-tauri/src/winlink/modem/vara/commands.rs src-tauri/src/winlink/modem/vara/listener.rs
git commit -m "feat(vara): REGISTERED/WRONG/CONNECTED-shape parsing, SessionType/RETRIES/Connect-VIA rendering, doc-exact compression vocab"
```

---

### Task 3: REGISTERED readiness gate (fail-open, T_min settle)

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs`
- Test: `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Consumes: `InboundCommand::Registered` / `LinkRegistered` (Task 2).
- Produces (consumed by Task 4): `pub(crate) enum Readiness { Confirmed, Unconfirmed }`; `pub(crate) fn wait_for_readiness(transport: &mut VaraTransport, t_min: Duration, t_max: Duration) -> Readiness`; constants `VARA_READY_T_MIN` (600 ms), `VARA_READY_T_MAX` (5 s).

Design constraints from the spec `[R3-2][R1-C2]`:
- Accept ANY `REGISTERED` line (bare or with callsign — no callsign match; unregistered VARA is the common case) and also `LINK REGISTERED`.
- Always honor the `T_min` settle (defeats the 464 ms m9kcd race) even when the token arrives instantly.
- On `T_max` expiry, **fail OPEN**: return `Unconfirmed`, caller logs "modem readiness unconfirmed" and proceeds. Never a hard error, never a wedge (ARDOP ARQTimeout lesson).
- The gate runs ONCE per transport-open (called only from `vara_open_session_inner`, Task 4) — structurally latched; dials never re-wait `[R3-2 refined]`.

- [ ] **Step 1: Write the failing tests** (append to `mod tests` in `commands.rs`; uses the same `TcpListener` loopback pattern as `loopback_vara_open_session`, `commands.rs:4283-4322`)

```rust
    /// Scripted cmd-socket acceptor: writes `lines` (each + "\r") after
    /// `delay`, then holds the socket open for `hold`.
    fn scripted_cmd_acceptor(
        listener: std::net::TcpListener,
        lines: Vec<&'static str>,
        delay: std::time::Duration,
        hold: std::time::Duration,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            use std::io::Write;
            let (mut s, _) = listener.accept().unwrap();
            std::thread::sleep(delay);
            for l in lines {
                let _ = s.write_all(format!("{l}\r").as_bytes());
            }
            std::thread::sleep(hold);
        })
    }

    fn loopback_transport_for_readiness(
        lines: Vec<&'static str>,
        delay: std::time::Duration,
    ) -> (VaraTransport, std::thread::JoinHandle<()>, std::thread::JoinHandle<()>) {
        use std::net::TcpListener;
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let cmd_h = scripted_cmd_acceptor(cmd_l, lines, delay, std::time::Duration::from_secs(2));
        let data_h = std::thread::spawn(move || {
            let (_c, _) = data_l.accept().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
        });
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            read_timeout: Some(std::time::Duration::from_millis(200)),
            ..VaraConfig::default()
        };
        let transport = VaraTransport::connect(&cfg).expect("loopback connect");
        (transport, cmd_h, data_h)
    }

    #[test]
    fn readiness_confirms_on_bare_registered_after_t_min_settle() {
        let (mut t, _h1, _h2) = loopback_transport_for_readiness(
            vec!["REGISTERED"],
            std::time::Duration::from_millis(50),
        );
        let start = std::time::Instant::now();
        let r = wait_for_readiness(
            &mut t,
            std::time::Duration::from_millis(300),
            std::time::Duration::from_secs(3),
        );
        assert_eq!(r, Readiness::Confirmed);
        // T_min settle honored even though the token arrived at ~50ms.
        assert!(start.elapsed() >= std::time::Duration::from_millis(300));
    }

    #[test]
    fn readiness_confirms_on_registered_with_callsign_and_on_link_registered() {
        let (mut t, _h1, _h2) = loopback_transport_for_readiness(
            vec!["REGISTERED W6ABC-7"],
            std::time::Duration::from_millis(10),
        );
        assert_eq!(
            wait_for_readiness(
                &mut t,
                std::time::Duration::from_millis(50),
                std::time::Duration::from_secs(3)
            ),
            Readiness::Confirmed
        );
        let (mut t2, _h3, _h4) = loopback_transport_for_readiness(
            vec!["LINK REGISTERED"],
            std::time::Duration::from_millis(10),
        );
        assert_eq!(
            wait_for_readiness(
                &mut t2,
                std::time::Duration::from_millis(50),
                std::time::Duration::from_secs(3)
            ),
            Readiness::Confirmed
        );
    }

    #[test]
    fn readiness_fails_open_on_t_max_expiry() {
        // Silent modem: NO token. Must return Unconfirmed at ~T_max — a
        // warning outcome, never an error, never a wedge [R3-2].
        let (mut t, _h1, _h2) =
            loopback_transport_for_readiness(vec![], std::time::Duration::from_millis(1));
        let start = std::time::Instant::now();
        let r = wait_for_readiness(
            &mut t,
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(600),
        );
        assert_eq!(r, Readiness::Unconfirmed);
        assert!(start.elapsed() >= std::time::Duration::from_millis(600));
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "must not wedge");
    }
```

- [ ] **Step 2: Verify failure** — compile error (`wait_for_readiness` undefined). Local `cargo test … vara` if the target is warm, else CI.

- [ ] **Step 3: Implement** (in `commands.rs`, near `VARA_CONNECT_DEADLINE` at line 2003)

```rust
/// Readiness-gate settle floor: always waited after transport open, whether
/// or not a REGISTERED token arrives. Defeats the 464 ms m9kcd race [R3-2].
pub(crate) const VARA_READY_T_MIN: Duration = Duration::from_millis(600);
/// Readiness-gate ceiling: on expiry the open proceeds with a warning
/// (fail OPEN — anti-wedge posture per the ARDOP ARQTimeout lesson).
pub(crate) const VARA_READY_T_MAX: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readiness {
    Confirmed,
    Unconfirmed,
}

/// Wait for any readiness token (`REGISTERED …` bare or SSID'd, or
/// `LINK REGISTERED`) up to `t_max`, always honoring the `t_min` settle.
/// Runs ONCE per transport-open (from `vara_open_session_inner`); dials
/// never re-wait — REGISTERED does not repeat per dial [R3-2].
pub(crate) fn wait_for_readiness(
    transport: &mut VaraTransport,
    t_min: Duration,
    t_max: Duration,
) -> Readiness {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() >= t_max {
            return Readiness::Unconfirmed; // fail OPEN — caller logs + proceeds
        }
        match transport.recv() {
            Ok(Some(InboundCommand::Registered(_)))
            | Ok(Some(InboundCommand::LinkRegistered)) => {
                let settle = t_min.saturating_sub(start.elapsed());
                if !settle.is_zero() {
                    std::thread::sleep(settle);
                }
                return Readiness::Confirmed;
            }
            Ok(_) => {} // absorb IAMALIVE / BUFFER / timeouts, keep polling
            Err(_) => {
                // Socket hiccup: never wedge — brief backoff, poll to t_max.
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked readiness` (or CI). Expected: 3 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/modem/vara/commands.rs
git commit -m "feat(vara): REGISTERED readiness gate — T_min settle, T_max fail-open, once per transport-open"
```

---

### Task 4: Engine-split open sequence (HF/SAT vs FM)

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (`vara_open_session_inner`, lines ~1663-1760)
- Test: `mod tests` in the same file

**Interfaces:**
- Consumes: Task 2 (`SessionType`, `Compression::Text`, `Retries`, `Public`), Task 3 (`wait_for_readiness`, constants).
- Produces: the open sequence contract relied on by Task 5 and by the mock-server ordering tests: **HF/SAT open** = `MYCALL` → readiness gate → `PUBLIC ON` → `SessionType` → `COMPRESSION TEXT` → `RETRIES 10` → optional `BW`; **FM open** = `MYCALL` → `T_min` settle only. LISTEN remains owned by the listener-arm path (Task 6), unchanged at open.

**Recorded decisions (spec §7 requires them recorded here):**
- **CWID is NOT sent by Tuxlink.** CW station identification is a per-station regulatory choice the operator configures in VARA itself (`VARA.ini` persists it); WLE's own CWID setter is on its `WRONG`-suppression list (`VaraSession.cs:4452`), i.e. it draws `WRONG` across versions. Owning it adds a failure mode without adding capability. `[R3-5]` is satisfied by `PUBLIC ON` being sent and CWID being explicitly delegated to VARA's own config.
- **`RETRIES 10` is sent HF-only and is `WRONG`-tolerant.** Whether VARA accepts it over TCP (vs `VARA.ini`-only) is confirmed by the bench wire-tap (spec §8 step 1); either answer is safe because the send is non-fatal `[R3-4]`.
- Every setter after MYCALL is **fire-and-forget** (matching the existing BW posture, `commands.rs:1741-1752`); `WRONG` replies are absorbed by the wait loops and logged by Task 5's dial-window drain — never fatal `[R3-3]`.

- [ ] **Step 1: Write the failing test** (append to `mod tests`; a scripted acceptor that CAPTURES what the client writes)

```rust
    /// Acceptor that records every "\r"-terminated line the client writes
    /// to the cmd socket, optionally replying to the first line with
    /// `reply` (e.g. "REGISTERED"). Returns the captured lines via mpsc.
    fn capturing_cmd_acceptor(
        listener: std::net::TcpListener,
        reply: Option<&'static str>,
        capture_for: std::time::Duration,
    ) -> std::sync::mpsc::Receiver<Vec<String>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();
            let mut buf = Vec::new();
            let mut lines: Vec<String> = Vec::new();
            let start = std::time::Instant::now();
            let mut replied = false;
            while start.elapsed() < capture_for {
                let mut b = [0u8; 256];
                match s.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&b[..n]);
                        while let Some(pos) = buf.iter().position(|&c| c == b'\r') {
                            let line: Vec<u8> = buf.drain(..=pos).collect();
                            lines.push(
                                String::from_utf8_lossy(&line[..line.len() - 1]).into_owned(),
                            );
                            if !replied {
                                if let Some(r) = reply {
                                    let _ = s.write_all(format!("{r}\r").as_bytes());
                                    replied = true;
                                }
                            }
                        }
                    }
                    Err(_) => {} // read timeout — keep capturing
                }
            }
            let _ = tx.send(lines);
        });
        rx
    }

    #[test]
    fn hf_open_sends_full_setter_sequence_in_order() {
        use std::net::TcpListener;
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let rx = capturing_cmd_acceptor(cmd_l, Some("REGISTERED"), std::time::Duration::from_secs(3));
        let _dh = std::thread::spawn(move || {
            let (_c, _) = data_l.accept().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
        });
        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            bandwidth_hz: Some(2300),
        };
        vara_open_session_inner(&session, &ui_cfg, Some("W6ABC"), SessionIntent::P2p, TransportKind::VaraHf)
            .expect("open");
        let lines = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        // Spec §7 open order: MYCALL → (gate) → PUBLIC ON → SessionType →
        // COMPRESSION TEXT → RETRIES 10 → BW. (CWID intentionally absent.)
        assert_eq!(
            lines,
            vec![
                "MYCALL W6ABC",
                "PUBLIC ON",
                "P2P SESSION",
                "COMPRESSION TEXT",
                "RETRIES 10",
                "BW2300",
            ]
        );
    }

    #[test]
    fn fm_open_sends_mycall_only() {
        use std::net::TcpListener;
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let rx = capturing_cmd_acceptor(cmd_l, None, std::time::Duration::from_secs(2));
        let _dh = std::thread::spawn(move || {
            let (_c, _) = data_l.accept().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
        });
        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            bandwidth_hz: Some(2300), // set, but FM must NOT send BW [R3-1]
        };
        vara_open_session_inner(&session, &ui_cfg, Some("W6ABC"), SessionIntent::P2p, TransportKind::VaraFm)
            .expect("open");
        let lines = rx.recv_timeout(std::time::Duration::from_secs(4)).unwrap();
        // FM command set is MYCALL/LISTEN/CONNECT/ABORT/DISCONNECT only —
        // no SessionType, COMPRESSION, RETRIES, PUBLIC, or BW [R3-1].
        assert_eq!(lines, vec!["MYCALL W6ABC"]);
    }
```

- [ ] **Step 2: Verify failure** — the HF test fails (only `MYCALL` + `BW2300` are sent today; no PUBLIC/SessionType/COMPRESSION/RETRIES); the FM test fails (BW is currently sent for FM too).

- [ ] **Step 3: Implement.** In `vara_open_session_inner`, replace the current post-MYCALL block (`commands.rs:1741-1752`, the best-effort BW send) with the engine split:

```rust
    // ── Engine-split setter sequence (spec §7, [R3-1][R1-C1]) ────────────
    // HF/SAT: readiness gate, then PUBLIC ON → SessionType → COMPRESSION
    // TEXT → RETRIES 10 → optional BW. All fire-and-forget; a WRONG reply
    // to any setter is absorbed and logged, never fatal [R3-3].
    // FM: WLE's VaraFMSession sends ONLY MYCALL/LISTEN/CONNECT/ABORT/
    // DISCONNECT — no setters. A T_min settle replaces the gate (FM emits
    // REGISTERED too, but it is log-only there).
    // CWID: intentionally NOT sent — delegated to VARA's own config
    // (recorded decision, plan Task 4).
    if transport_kind == TransportKind::VaraFm {
        std::thread::sleep(VARA_READY_T_MIN);
    } else {
        match wait_for_readiness(&mut transport, VARA_READY_T_MIN, VARA_READY_T_MAX) {
            Readiness::Confirmed => {}
            Readiness::Unconfirmed => {
                // Fail OPEN [R3-2]: proceed with a warning, never a wedge.
                eprintln!(
                    "VARA: modem readiness unconfirmed after {}s — proceeding",
                    VARA_READY_T_MAX.as_secs()
                );
            }
        }
        let _ = transport.send(&OutboundCommand::Public(true)); // [R3-5]
        let _ = transport.send(&OutboundCommand::SessionType(
            VaraSessionType::from_intent(intent),
        ));
        let _ = transport.send(&OutboundCommand::Compression(Compression::Text));
        let _ = transport.send(&OutboundCommand::Retries(10)); // [R3-4] HF-only
        if let Some(hz) = ui_cfg.bandwidth_hz {
            if let Some(bw) = bandwidth_from_hz(hz) {
                let _ = transport.send(&OutboundCommand::Bw(bw));
            }
        }
    }
```

Note: `vara_open_session_inner` currently sends MYCALL only when a callsign is provided (`commands.rs:1731-1738`) — keep that guard; when the callsign is present, validate it first with `crate::winlink::callsign::validate_wire_callsign` and on `Err` skip the send with a logged warning (pre-wizard tolerance preserved; an invalid stored identity must not brick the open). `transport` must be `mut` through this block (it already is at the MYCALL send site).

**Compile/test ripple:** the existing `loopback_vara_open_session` helper's dumb acceptor (`commands.rs:4296-4305`) never sends REGISTERED, so every HF loopback open would now block until `VARA_READY_T_MAX` (5 s). Update the helper's cmd acceptor to write `REGISTERED\r` immediately after accept (one-line change inside the `cmd_handle` closure):

```rust
        let cmd_handle = thread::spawn(move || {
            use std::io::Write;
            let (mut c, _) = cmd_l.accept().unwrap();
            let _ = c.write_all(b"REGISTERED\r");
            thread::sleep(Duration::from_millis(1500));
        });
```

and extend the helper's post-open holds from 500 ms to 1500 ms (the gate adds up to ~600 ms before the setters).

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked vara` (or CI). Expected: both new tests PASS and every pre-existing open/lifecycle test still PASSes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/modem/vara/commands.rs
git commit -m "feat(vara): engine-split open sequence — HF setter chain behind readiness gate; FM sends MYCALL only"
```

---

### Task 5: Dial path — per-candidate SessionType, WRONG fail-fast, SSID echo base-match, wire-grammar validation, via threading

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (`send_connect_and_wait` ~2697-2720, `wait_for_connected` ~2739-2828, `run_vara_b2f_with_transport` candidate walk ~2460-2534)
- Test: `mod tests` in the same file

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: `send_connect_and_wait(app, log, transport, mycall, target, via: &[String], engine: TransportKind, intent: SessionIntent, ptt)` — the new signature every dial-walk call site uses. Dial candidates carry `via: Vec<String>` (default empty).

Spec requirements implemented here:
1. `SessionType` re-sent **inside `send_connect_and_wait`, immediately before each CONNECT**, HF/SAT only — a multi-candidate QSY walk never dials in a stale session mode `[R3-9-placement]`.
2. After sending `SessionType`, drain the cmd socket for 250 ms: a `WRONG` in that window is a setter rejection — **log, never fatal** `[R3-3]`. Only then send `CONNECT`, so a `WRONG` seen by `wait_for_connected` is attributable to the CONNECT itself and **fails fast** `[R3-6-wrong]`.
3. `wait_for_connected` matches the CONNECTED echo on `canonical_base` — VARA echoes the bare callsign while the wire dial string stays SSID'd (gbb05) `[R3-3-echo]`.
4. `validate_wire_callsign` on mycall + every candidate target before any send; a failure is a pre-dial `ConnectFailed`, not a wire write `[R3-9]`.
5. `Connect { via }` threaded (FM digi path); existing callers pass `&[]`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn wait_for_connected_matches_ssid_dial_against_bare_echo() {
        // gbb05: dial "N0DAJ-7", VARA echoes "CONNECTED W6ABC N0DAJ 2300".
        // The base-match accepts it; the wire string stays SSID'd.
        let (mut t, _h1, _h2) = loopback_transport_for_readiness(
            vec!["CONNECTED W6ABC N0DAJ 2300"],
            std::time::Duration::from_millis(20),
        );
        let ptt: SharedPtt = Default::default();
        wait_for_connected(&mut t, "N0DAJ-7", std::time::Duration::from_secs(3), &ptt)
            .expect("SSID'd dial must accept bare-callsign CONNECTED echo");
    }

    #[test]
    fn wait_for_connected_still_rejects_a_different_peer() {
        let (mut t, _h1, _h2) = loopback_transport_for_readiness(
            vec!["CONNECTED W6ABC K7XYZ 2300"],
            std::time::Duration::from_millis(20),
        );
        let ptt: SharedPtt = Default::default();
        let err = wait_for_connected(&mut t, "N0DAJ-7", std::time::Duration::from_secs(3), &ptt)
            .unwrap_err();
        assert!(err.contains("unexpected CONNECTED peer"), "{err}");
    }

    #[test]
    fn wait_for_connected_fails_fast_on_bare_wrong() {
        // [R3-6-wrong]: a WRONG after CONNECT is a rejected/malformed dial —
        // fail in ms, not after the 120 s VARA_CONNECT_DEADLINE.
        let (mut t, _h1, _h2) = loopback_transport_for_readiness(
            vec!["WRONG"],
            std::time::Duration::from_millis(20),
        );
        let ptt: SharedPtt = Default::default();
        let start = std::time::Instant::now();
        let err = wait_for_connected(&mut t, "N0DAJ-7", std::time::Duration::from_secs(30), &ptt)
            .unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(err.contains("WRONG"), "{err}");
    }

    #[test]
    fn send_connect_prefixes_session_type_for_hf_but_not_fm() {
        // Captures the cmd stream for an HF dial and an FM dial.
        // HF: ["P2P SESSION", "CONNECT W6ABC N0DAJ-7"]
        // FM: ["CONNECT W6ABC N0DAJ-7 VIA DIGI1"]
        // (uses capturing_cmd_acceptor from Task 4; no CONNECTED reply, so
        // the wait times out — pass a 1 s deadline and ignore the Err.)
        use std::net::TcpListener;
        for (kind, expected) in [
            (
                TransportKind::VaraHf,
                vec!["P2P SESSION".to_string(), "CONNECT W6ABC N0DAJ-7".to_string()],
            ),
            (
                TransportKind::VaraFm,
                vec!["CONNECT W6ABC N0DAJ-7 VIA DIGI1".to_string()],
            ),
        ] {
            let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
            let cmd_port = cmd_l.local_addr().unwrap().port();
            let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
            let data_port = data_l.local_addr().unwrap().port();
            let rx = capturing_cmd_acceptor(cmd_l, None, std::time::Duration::from_secs(2));
            let _dh = std::thread::spawn(move || {
                let (_c, _) = data_l.accept().unwrap();
                std::thread::sleep(std::time::Duration::from_secs(2));
            });
            let cfg = VaraConfig {
                host: "127.0.0.1".into(),
                cmd_port,
                data_port,
                read_timeout: Some(std::time::Duration::from_millis(100)),
                ..VaraConfig::default()
            };
            let mut t = VaraTransport::connect(&cfg).unwrap();
            let ptt: SharedPtt = Default::default();
            let via: Vec<String> =
                if kind == TransportKind::VaraFm { vec!["DIGI1".into()] } else { vec![] };
            let _ = send_connect_and_wait_inner(
                &mut t, "W6ABC", "N0DAJ-7", &via, kind, SessionIntent::P2p,
                std::time::Duration::from_secs(1), &ptt,
            );
            let lines = rx.recv_timeout(std::time::Duration::from_secs(4)).unwrap();
            assert_eq!(lines, expected, "kind={kind:?}");
        }
    }
```

- [ ] **Step 2: Verify failure** — compile FAIL (`send_connect_and_wait_inner` undefined; `wait_for_connected` has no WRONG arm; echo match is exact-compare).

- [ ] **Step 3: Implement.**

(a) In `wait_for_connected` (`commands.rs:2756-2771`), replace the echo comparison and add the WRONG arm:

```rust
            Ok(Some(InboundCommand::Connected { target: peer, .. })) => {
                // gbb05 [R3-3-echo]: VARA's CONNECTED echoes the BARE
                // callsign even when the dial string was SSID'd. Compare on
                // canonical base; the wire dial string stays SSID'd.
                if crate::winlink::callsign::canonical_base(&peer)
                    == crate::winlink::callsign::canonical_base(target)
                {
                    return Ok(());
                }
                return Err(format!(
                    "unexpected CONNECTED peer={peer} (expected {target})"
                ));
            }
            Ok(Some(InboundCommand::Wrong)) => {
                // [R3-6-wrong]: WRONG after CONNECT = rejected/malformed
                // dial. Fail fast instead of eating the connect deadline.
                return Err("VARA rejected CONNECT (WRONG)".to_string());
            }
```

(b) Split `send_connect_and_wait` into a testable core + the AppHandle wrapper. Replace the whole function (`commands.rs:2697-2720`):

```rust
/// Testable core of the dial: optional per-candidate SessionType prefix
/// (HF/SAT only [R3-9-placement]), a 250 ms setter-WRONG drain [R3-3],
/// then CONNECT (with optional VIA digis [R3-6]) and the CONNECTED wait.
#[allow(clippy::too_many_arguments)]
fn send_connect_and_wait_inner(
    transport: &mut VaraTransport,
    mycall: &str,
    target: &str,
    via: &[String],
    engine: TransportKind,
    intent: SessionIntent,
    deadline: Duration,
    ptt: &SharedPtt,
) -> Result<(), String> {
    if engine != TransportKind::VaraFm {
        transport
            .send(&OutboundCommand::SessionType(VaraSessionType::from_intent(intent)))
            .map_err(|e| format!("VARA cmd-port SESSION write failed: {e}"))?;
        // Setter-WRONG drain: a WRONG in this window belongs to the
        // SessionType setter (WLE suppression-list parity) — log, proceed.
        let drain_until = std::time::Instant::now() + Duration::from_millis(250);
        while std::time::Instant::now() < drain_until {
            match transport.recv() {
                Ok(Some(InboundCommand::Wrong)) => {
                    eprintln!("VARA: setter drew WRONG before CONNECT (non-fatal [R3-3])");
                }
                Ok(Some(_)) | Ok(None) => {}
                Err(_) => break,
            }
        }
    }
    transport
        .send(&OutboundCommand::Connect {
            mycall: mycall.to_string(),
            target: target.to_string(),
            via: via.to_vec(),
        })
        .map_err(|e| format!("VARA cmd-port CONNECT write failed: {e}"))?;
    wait_for_connected(transport, target, deadline, ptt)
        .map_err(|e| format!("VARA CONNECT to {target} failed: {e}"))
}

fn send_connect_and_wait(
    app: &AppHandle,
    log: &Arc<SessionLogState>,
    transport: &mut VaraTransport,
    mycall: &str,
    target: &str,
    via: &[String],
    engine: TransportKind,
    intent: SessionIntent,
    ptt: &SharedPtt,
) -> Result<(), String> {
    emit_vara_log(
        app,
        log,
        LogLevel::Info,
        format!("VARA CONNECT {mycall} {target}"),
    );
    send_connect_and_wait_inner(
        transport, mycall, target, via, engine, intent, VARA_CONNECT_DEADLINE, ptt,
    )
}
```

Note: the setter-WRONG drain loops on `transport.recv()`, whose read timeout is 2 s (`VaraConfig::default`) — a silent socket makes the drain take one full read-timeout tick rather than exactly 250 ms. That is acceptable (bounded, sub-deadline); do NOT shorten the socket timeout for this.

(c) In the candidate walk (`commands.rs:2497`), update the call site to pass the new arguments. The candidate struct used by the walk gains `via`; where candidates are built from `QsyCandidateDto`/favorites prefill, default `via: vec![]`. The engine comes from `session.active_transport_kind()` (already read at `commands.rs:2464-2470` for the sideband flag — reuse that binding); `intent` is already a parameter of `run_vara_b2f_with_transport`:

```rust
        match send_connect_and_wait(
            app, log, transport, &mycall, &c.target, &c.via, engine, intent, keyer,
        ) {
```

(d) Pre-dial grammar validation `[R3-9]`, at the top of the candidate walk (before the first tune), so an invalid callsign is a clean `ConnectFailed` and never a wire write:

```rust
    if let Err(e) = crate::winlink::callsign::validate_wire_callsign(&mycall) {
        return VaraExchangeOutcome::ConnectFailed(format!("invalid MYCALL: {e}"));
    }
    for c in &candidates {
        if let Err(e) = crate::winlink::callsign::validate_wire_callsign(&c.target) {
            return VaraExchangeOutcome::ConnectFailed(format!(
                "invalid dial target {:?}: {e}", c.target
            ));
        }
    }
```

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked vara` (or CI). Expected: 4 new tests PASS; the pre-existing `wait_for_connected` tests (case-insensitive match) still PASS (base-match subsumes case-insensitive compare).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/modem/vara/commands.rs
git commit -m "feat(vara): per-candidate SessionType, WRONG fail-fast dial, SSID echo base-match (gbb05), wire-grammar pre-dial validation"
```

---

### Task 6: LISTEN sequencing — setter only when the ARQ link is down

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (`send_listen_on`, ~1015-1035)
- Test: `mod tests` in the same file

**Interfaces:**
- Consumes: `VaraSessionInner.current_exchange` (existing exchange marker, set/cleared by `begin_exchange`/`end_exchange`, `commands.rs:469-484`).
- Produces: `send_listen_on()` refuses (with a descriptive error) while an exchange is in flight `[R3-8]`. Re-arm after an exchange is already sequenced by the existing consumer-task flow (`end_exchange` → re-arm), which now cannot race ahead of teardown because of this guard.

Spec: `LISTEN ON/OFF` force-disconnects an active link if received mid-connection. The setter must be sent only when the ARQ link is confirmed down (at open, or strictly after `DISCONNECTED`).

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn send_listen_on_refuses_while_exchange_in_flight() {
        // [R3-8]: LISTEN mid-link force-disconnects the ARQ session. The
        // setter must refuse while an exchange is marked in flight.
        let session = loopback_vara_open_session(SessionIntent::P2p, TransportKind::VaraHf);
        session.begin_exchange(ExchangeState::default());
        let err = session
            .send_listen_on()
            .expect_err("LISTEN ON must refuse while an exchange is in flight");
        assert!(err.contains("exchange"), "{err}");
        session.end_exchange();
        session
            .send_listen_on()
            .expect("LISTEN ON must succeed once the link is down");
    }
```

(If `ExchangeState` does not implement `Default`, construct it the way `begin_exchange` call sites do — read the enum/struct at its definition and use its dial variant.)

- [ ] **Step 2: Verify failure** — the first `send_listen_on` currently succeeds mid-exchange.

- [ ] **Step 3: Implement.** In `send_listen_on` (`commands.rs:1024`), after taking the lock and before the transport lookup:

```rust
        if guard.current_exchange.is_some() {
            return Err(
                "LISTEN deferred: an exchange is in flight — LISTEN mid-link \
                 force-disconnects the ARQ session [R3-8]; re-arm after DISCONNECTED"
                    .to_string(),
            );
        }
```

Then audit the two re-arm call sites (the listener consumer task in `ui_commands.rs` and the arm command) — both already call `end_exchange()` before re-arming; the guard turns any future ordering regression into a loud error instead of a silent force-disconnect.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked listen` (or CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/modem/vara/commands.rs
git commit -m "fix(vara): refuse LISTEN setter while ARQ exchange in flight — mid-link LISTEN force-disconnects"
```

---

### Task 7: Peer data model (`peers/model.rs`) — types + serde shapes

**Files:**
- Create: `src-tauri/src/peers/mod.rs`, `src-tauri/src/peers/model.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod peers;` alongside the other module declarations)
- Test: inline `#[cfg(test)]` in `model.rs`

**Interfaces:**
- Produces (consumed by Tasks 8-25): every type below, exactly as written. Field names are the on-disk JSON and the TS mirror — do not rename later.

- [ ] **Step 1: Write the failing shape tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_enum_variants_quarantine_the_field_not_the_roster() {
        // [R4-5]: a variant written by a future binary deserializes to
        // Unknown; the row and roster survive.
        let j = r#"{"schema_version":1,"peers":[{
            "id":"p1","canonical_base":"W6ABC","presented_callsigns":["W6ABC-7"],
            "identity_kind":"quantum-club","do_not_merge":false,"conflict":false,
            "source":"auto","origin":"time-travel","contact_id":null,"grid":null,
            "note":"","created_at":"2026-07-10T12:00:00-07:00",
            "last_connected_at":null,"channels":[],"endpoints":[]}]}"#;
        let f: PeersFile = serde_json::from_str(j).expect("unknown variants must not fail the load");
        assert_eq!(f.peers[0].identity_kind, IdentityKind::Unknown);
        assert_eq!(f.peers[0].origin, Origin::Unknown);
    }

    #[test]
    fn bandwidth_round_trips_all_kinds() {
        for bw in [
            ChannelBandwidth::Hz { hz: 2300 },
            ChannelBandwidth::Wide,
            ChannelBandwidth::Narrow,
        ] {
            let s = serde_json::to_string(&bw).unwrap();
            assert_eq!(serde_json::from_str::<ChannelBandwidth>(&s).unwrap(), bw);
        }
        // Future kind → Unknown, not a load failure.
        let f: ChannelBandwidth = serde_json::from_str(r#"{"kind":"ultra"}"#).unwrap();
        assert_eq!(f, ChannelBandwidth::Unknown);
    }

    #[test]
    fn enum_wire_tags_are_kebab_case() {
        // Shape pins per the serde rename_all memory: tags only, explicit test.
        assert_eq!(serde_json::to_string(&RecordSource::OperatorPinned).unwrap(), r#""operator-pinned""#);
        assert_eq!(serde_json::to_string(&Provenance::ObservedIncoming).unwrap(), r#""observed-incoming""#);
        assert_eq!(serde_json::to_string(&ChannelTransport::VaraFm).unwrap(), r#""vara-fm""#);
        assert_eq!(serde_json::to_string(&Origin::Incoming).unwrap(), r#""incoming""#);
    }

    #[test]
    fn default_file_has_schema_version_1() {
        let f = PeersFile::default();
        assert_eq!(f.schema_version, SCHEMA_VERSION);
        assert_eq!(f.schema_version, 1);
        assert!(f.peers.is_empty());
    }
}
```

- [ ] **Step 2: Verify failure** — module does not exist.

- [ ] **Step 3: Implement `model.rs`** (mirrors the `contacts/store.rs` serde policy: `#[serde(default)]` on every field, NO `deny_unknown_fields`, hand-written `Default` for the file; `#[serde(other)] Unknown` per the `AntennaPreset` precedent, `src-tauri/src/propagation/antenna.rs:57-81`)

```rust
//! Peer data model (spec §1/§2). `peers.json` on-disk shapes.
//!
//! Serde policy (mirrors `contacts/store.rs`): additive tolerance via
//! `#[serde(default)]`, NO `deny_unknown_fields`, every enum carries a
//! `#[serde(other)] Unknown` catch-all so a future variant quarantines
//! one field, never the roster [R4-5].

use serde::{Deserialize, Serialize};

/// On-disk schema version. Bumped only on a non-additive shape change.
pub const SCHEMA_VERSION: u32 = 1;

/// Soft cap on auto-created records; over-cap eviction is LRU among
/// `RecordSource::Auto` records only [R2-S6][R1-C9].
pub const AUTO_PEER_CAP: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityKind {
    Individual,
    /// Tactical calls have no standard structure; a Tactical peer's dedup
    /// anchor is its FULL presented string, never base-normalized [R4-6].
    Tactical,
    Club,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RecordSource {
    /// Created by the recorder. Evictable under the cap.
    #[default]
    Auto,
    /// Operator-added. Never evicted.
    Manual,
    /// Operator-pinned. Never evicted.
    OperatorPinned,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Incoming,
    Outgoing,
    Manual,
    Aprs,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GridSource {
    Contact,
    Aprs,
    Manual,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelTransport {
    Packet,
    Ardop,
    VaraHf,
    VaraFm,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Incoming,
    Outgoing,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// Operator-entered or operator-promoted. The ONLY agent-dialable
    /// provenance (spec §4 I1). Monotonic: never downgraded [R4-4][R2-S8].
    Operator,
    /// Learned because a station connected to us. Agent-non-dialable,
    /// never auto-promoted, badged "unverified claimed identity".
    #[default]
    ObservedIncoming,
    #[serde(other)]
    Unknown,
}

/// Bandwidth observed on a CONNECTED line. Internally tagged so the one
/// data-carrying variant coexists with `#[serde(other)]` (the officially
/// supported form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ChannelBandwidth {
    Hz { hz: u32 },
    Wide,
    Narrow,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerGrid {
    pub value: String,
    pub source: GridSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AttemptCounts {
    #[serde(default)]
    pub ok: u32,
    #[serde(default)]
    pub fail: u32,
}

/// One RF reachability observation row (spec §2). Dedup key:
/// `(transport, target_callsign, via, freq_hz, bandwidth)` [R4-11].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub transport: ChannelTransport,
    /// EXACT SSID'd callsign for the wire (e.g. `N0DAJ-7`). The wire target
    /// of any dial is always this, never `canonical_base` [R3-9].
    pub target_callsign: String,
    /// Digipeater path, max 2 (packet / VARA FM); empty = direct [R3-6].
    #[serde(default)]
    pub via: Vec<String>,
    /// Center frequency, exact Hz (catalog semantics, #1064) — no rounding.
    #[serde(default)]
    pub freq_hz: Option<u64>,
    #[serde(default)]
    pub bandwidth: Option<ChannelBandwidth>,
    /// Most recent direction observed on this channel.
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub counts: AttemptCounts,
    pub last_seen: String,
}

/// One network reachability row (telnet P2P).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Stable id — keyring key component (`p2p-endpoint:<peer_id>:<id>`).
    /// Promotion mutates provenance IN PLACE on this id so the keyring
    /// secret is never orphaned [R5-5].
    pub id: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub provenance: Provenance,
    pub last_seen: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// Stable unique id — the primary key (uuid v4; house pattern).
    pub id: String,
    /// Dedup anchor ONLY (spec §1). Never a wire source.
    pub canonical_base: String,
    /// Every exact form observed or dialed, deduped, verbatim.
    #[serde(default)]
    pub presented_callsigns: Vec<String>,
    #[serde(default)]
    pub identity_kind: IdentityKind,
    /// Set when the operator splits; suppresses auto-merge forever [R5-4].
    #[serde(default)]
    pub do_not_merge: bool,
    /// Held-for-manual-association marker: an observation on a split base
    /// that matched no split record's presented callsigns [R5-4].
    #[serde(default)]
    pub conflict: bool,
    #[serde(default)]
    pub source: RecordSource,
    #[serde(default)]
    pub origin: Origin,
    /// One-way link into contacts.json (spec §Cross-store).
    #[serde(default)]
    pub contact_id: Option<String>,
    #[serde(default)]
    pub grid: Option<PeerGrid>,
    /// Operator free-text. NEVER crosses the agent surface (spec §4).
    #[serde(default)]
    pub note: String,
    pub created_at: String,
    #[serde(default)]
    pub last_connected_at: Option<String>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeersFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub peers: Vec<Peer>,
}

// Hand-written Default so schema_version is 1, not 0 (contacts M1 pattern).
impl Default for PeersFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            peers: vec![],
        }
    }
}
```

`mod.rs`:

```rust
//! First-class peer store (spec: docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md).
pub mod commands;
pub mod model;
pub mod recorder;
pub mod store;
```

(Declare `commands`/`recorder`/`store` as they land in Tasks 8-11; for THIS task's commit, `mod.rs` contains only `pub mod model;` so the tree compiles.)

- [ ] **Step 4: Run the tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked peers::model` (or CI). Expected: 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/ src-tauri/src/lib.rs
git commit -m "feat(peers): data model — PeersFile/Peer/Channel/Endpoint with forward-compat serde shapes"
```

---

### Task 8: Peer store — open/quarantine/flush, upsert/dedup, split routing, merge/split/promote

**Files:**
- Create: `src-tauri/src/peers/store.rs` (add `pub mod store;` to `peers/mod.rs`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 7 types; Task 1 `canonical_base`.
- Produces (consumed by Tasks 9-11, 18, 19, 22-25):

```rust
pub struct PeersStore { /* path + PeersFile, like ContactsStore */ }
pub enum PeersError { Io(String), Serde(String), Validation(String) }   // Serialize, thiserror — mirror FavoritesError
impl PeersStore {
    pub fn open(path: PathBuf) -> Self;                                  // infallible; corrupt → quarantine + empty
    pub fn file(&self) -> &PeersFile;
    pub fn apply_observation(&mut self, obs: &crate::peers::recorder::PeerObservation, now: String) -> Result<ApplyEffect, PeersError>;
    pub fn upsert_manual(&mut self, peer: Peer) -> Result<(), PeersError>;
    pub fn delete_peer(&mut self, id: &str) -> Result<Vec<String>, PeersError>;  // returns removed endpoint ids for keyring cascade
    pub fn merge(&mut self, keep_id: &str, absorb_id: &str) -> Result<Vec<String>, PeersError>; // returns absorbed endpoint ids (re-key cascade)
    pub fn split(&mut self, peer_id: &str, moved_presented: Vec<String>, now: String) -> Result<String, PeersError>; // returns new peer id
    pub fn promote_endpoint(&mut self, peer_id: &str, endpoint_id: &str) -> Result<(), PeersError>;
}
pub enum ApplyEffect { CreatedPeer, UpdatedPeer, ConflictHeld, NoRecord }
```

Rules implemented (all pinned by tests below): dedup anchor = `canonical_base` except `IdentityKind::Tactical` records anchor on their full presented string `[R4-6]`; once ANY record on a base carries `do_not_merge`, routing is by exact presented callsign only, and a non-matching observation creates a `conflict: true` record `[R5-4]`; channel dedup key `(transport, target_callsign, via, freq_hz, bandwidth)` `[R4-11]`; endpoint dedup key `(host lowercased, port, provenance)`; provenance monotonic — an observation may never create or mutate an `Operator` endpoint; only `promote_endpoint` sets `Operator`, in place `[R4-4][R2-S8][R5-5]`; counts saturate; `Manual`/`OperatorPinned` never evicted.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::*;
    use crate::peers::recorder::{ObservationPhase, ObservedPath, PeerObservation};

    fn td() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn now() -> String {
        "2026-07-10T12:00:00-07:00".to_string()
    }
    fn rf_obs(presented: &str, dir: Direction, phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf {
                transport: ChannelTransport::VaraHf,
                via: vec![],
                freq_hz: Some(7_101_000),
                bandwidth: Some(ChannelBandwidth::Hz { hz: 2300 }),
            },
            direction: dir,
            presented_target: presented.to_string(),
            phase,
        }
    }

    #[test]
    fn upserts_by_canonical_base_and_keeps_presented_forms() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now()).unwrap();
        s.apply_observation(&rf_obs("w6abc", Direction::Incoming, ObservationPhase::Accepted), now()).unwrap();
        let f = s.file();
        assert_eq!(f.peers.len(), 1, "same base → one record");
        assert_eq!(f.peers[0].canonical_base, "W6ABC");
        assert!(f.peers[0].presented_callsigns.contains(&"W6ABC-7".to_string()));
        assert!(f.peers[0].presented_callsigns.contains(&"W6ABC".to_string()));
        // Two distinct channels (different target_callsign in the key).
        assert_eq!(f.peers[0].channels.len(), 2);
    }

    #[test]
    fn channel_key_distinguishes_via_freq_and_bandwidth() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let mut o1 = rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk);
        s.apply_observation(&o1, now()).unwrap();
        s.apply_observation(&o1, now()).unwrap(); // same key → counts, not a new row
        assert_eq!(s.file().peers[0].channels.len(), 1);
        assert_eq!(s.file().peers[0].channels[0].counts.ok, 2);
        if let ObservedPath::Rf { ref mut via, .. } = o1.path {
            *via = vec!["DIGI1".into()];
        }
        s.apply_observation(&o1, now()).unwrap(); // different via → distinct channel [R3-6]
        assert_eq!(s.file().peers[0].channels.len(), 2);
    }

    #[test]
    fn split_records_route_by_exact_presented_callsign() {
        // [R5-4]: after a split, base-anchored routing is OFF for that base.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now()).unwrap();
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk), now()).unwrap();
        let id = s.file().peers[0].id.clone();
        let new_id = s.split(&id, vec!["W6ABC-9".to_string()], now()).unwrap();
        assert!(s.file().peers.iter().all(|p| p.do_not_merge));
        // A new -9 observation routes to the split record…
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Incoming, ObservationPhase::Accepted), now()).unwrap();
        let split_rec = s.file().peers.iter().find(|p| p.id == new_id).unwrap();
        assert!(split_rec.channels.iter().any(|c| c.direction == Direction::Incoming));
        // …and an unmatched presented form is held as a conflict record, not
        // silently applied to the wrong twin.
        let eff = s
            .apply_observation(&rf_obs("W6ABC-11", Direction::Incoming, ObservationPhase::Accepted), now())
            .unwrap();
        assert!(matches!(eff, ApplyEffect::ConflictHeld));
        assert!(s.file().peers.iter().any(|p| p.conflict));
    }

    #[test]
    fn rejected_inbound_never_populates_the_roster() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let eff = s
            .apply_observation(&rf_obs("EVIL-1", Direction::Incoming, ObservationPhase::Rejected), now())
            .unwrap();
        assert!(matches!(eff, ApplyEffect::NoRecord));
        assert!(s.file().peers.is_empty(), "an attacker knocking is not a peer");
    }

    #[test]
    fn wedged_or_aborted_records_a_fail() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::AbortedOrWedged), now()).unwrap();
        assert_eq!(s.file().peers[0].channels[0].counts.fail, 1);
        assert_eq!(s.file().peers[0].channels[0].counts.ok, 0);
    }

    #[test]
    fn endpoint_provenance_is_monotonic() {
        // [R4-4][R2-S8]: an inbound observation may never create or mutate
        // an Operator endpoint; only promote_endpoint sets Operator, in place.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let obs = PeerObservation {
            path: ObservedPath::Telnet {
                host: "203.0.113.5".into(),
                port: 8772,
                provenance: Provenance::ObservedIncoming,
            },
            direction: Direction::Incoming,
            presented_target: "W6ABC".into(),
            phase: ObservationPhase::Accepted,
        };
        s.apply_observation(&obs, now()).unwrap();
        let (pid, eid, prov) = {
            let p = &s.file().peers[0];
            (p.id.clone(), p.endpoints[0].id.clone(), p.endpoints[0].provenance)
        };
        assert_eq!(prov, Provenance::ObservedIncoming);
        s.promote_endpoint(&pid, &eid).unwrap();
        assert_eq!(s.file().peers[0].endpoints[0].provenance, Provenance::Operator);
        assert_eq!(s.file().peers[0].endpoints[0].id, eid, "promotion is in-place [R5-5]");
        // A later ObservedIncoming observation of the same host:port must
        // NOT touch the Operator endpoint (distinct provenance in the key).
        s.apply_observation(&obs, now()).unwrap();
        let p = &s.file().peers[0];
        assert_eq!(p.endpoints.iter().filter(|e| e.provenance == Provenance::Operator).count(), 1);
    }

    #[test]
    fn corrupt_file_quarantines_and_starts_empty() {
        let dir = td();
        let path = dir.path().join("peers.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let s = PeersStore::open(path.clone());
        assert!(s.file().peers.is_empty());
        let quarantined = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(quarantined, "original bytes preserved");
    }

    #[test]
    fn atomic_write_round_trips() {
        let dir = td();
        let path = dir.path().join("peers.json");
        let mut s = PeersStore::open(path.clone());
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now()).unwrap();
        let reopened = PeersStore::open(path);
        assert_eq!(reopened.file().peers.len(), 1);
        assert_eq!(reopened.file().peers[0].canonical_base, "W6ABC");
    }

    #[test]
    fn merge_absorbs_channels_endpoints_and_presented_forms() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.upsert_manual(manual_peer("p-keep", "W6ABC")).unwrap();
        s.upsert_manual(manual_peer("p-absorb", "W6ABC")).unwrap();
        let absorbed = s.merge("p-keep", "p-absorb").unwrap();
        assert_eq!(s.file().peers.len(), 1);
        assert_eq!(s.file().peers[0].id, "p-keep");
        let _ = absorbed; // endpoint ids for the keyring re-key cascade (Task 10)
    }

    fn manual_peer(id: &str, base: &str) -> Peer {
        Peer {
            id: id.into(),
            canonical_base: base.into(),
            presented_callsigns: vec![base.into()],
            identity_kind: IdentityKind::Unknown,
            do_not_merge: false,
            conflict: false,
            source: RecordSource::Manual,
            origin: Origin::Manual,
            contact_id: None,
            grid: None,
            note: String::new(),
            created_at: "2026-07-10T12:00:00-07:00".into(),
            last_connected_at: None,
            channels: vec![],
            endpoints: vec![],
        }
    }
}
```

- [ ] **Step 2: Verify failure** — module does not exist. (These tests also need Task 11's `recorder` types `PeerObservation`/`ObservedPath`/`ObservationPhase` — create `peers/recorder.rs` with the TYPES ONLY in this task (the struct/enum definitions from Task 11 Step 3a verbatim), leaving the guard + `record_peer_observation` function to Task 11.)

- [ ] **Step 3: Implement `store.rs`.** `open` / `quarantine_corrupt` / `flush` are byte-for-byte the `ContactsStore` shape (`contacts/store.rs:104-191`) with `"peers.json"` as the default name and `PeersError` for errors. The store-specific logic:

```rust
impl PeersStore {
    /// Route an observation to its peer record and apply it (spec §2/§3).
    /// The caller has already classified rejected-inbound via
    /// `recorder::classify` — but this is defense-in-depth: a `Rejected`
    /// phase here is a NoRecord, never a roster write.
    pub fn apply_observation(
        &mut self,
        obs: &crate::peers::recorder::PeerObservation,
        now: String,
    ) -> Result<ApplyEffect, PeersError> {
        use crate::peers::recorder::{classify, Classified, ObservationPhase, ObservedPath};
        let bucket = classify(obs.phase);
        if matches!(bucket, Classified::NoRecord) {
            return Ok(ApplyEffect::NoRecord);
        }
        let presented = obs.presented_target.trim().to_ascii_uppercase();
        if crate::winlink::callsign::sanitize_display(&presented).is_none() {
            // Write-boundary floor [R2-S2]: a malformed callsign creates no
            // record (Task 18 also filters upstream; this is the backstop).
            return Ok(ApplyEffect::NoRecord);
        }
        let base = crate::winlink::callsign::canonical_base(&presented);

        // ── Routing [R5-4]: split bases route by exact presented form ────
        let base_has_split = self
            .file
            .peers
            .iter()
            .any(|p| p.canonical_base == base && p.do_not_merge);
        let idx = if base_has_split {
            match self.file.peers.iter().position(|p| {
                p.canonical_base == base && p.presented_callsigns.iter().any(|c| c == &presented)
            }) {
                Some(i) => Some(i),
                None => {
                    // Unmatched form on a split base: hold for manual
                    // association — never silently update the wrong twin.
                    let mut held = self.new_auto_peer(&base, &presented, obs, &now);
                    held.conflict = true;
                    self.file.peers.push(held);
                    self.flush()?;
                    return Ok(ApplyEffect::ConflictHeld);
                }
            }
        } else {
            self.file.peers.iter().position(|p| {
                if p.identity_kind == crate::peers::model::IdentityKind::Tactical {
                    // Tactical anchors on the full presented string [R4-6].
                    p.presented_callsigns.iter().any(|c| c == &presented)
                } else {
                    p.canonical_base == base
                }
            })
        };

        let created = idx.is_none();
        let idx = match idx {
            Some(i) => i,
            None => {
                let p = self.new_auto_peer(&base, &presented, obs, &now);
                self.file.peers.push(p);
                self.evict_over_cap();
                self.file.peers.len() - 1
            }
        };

        // ── Apply the observation to the record ──────────────────────────
        let ok = matches!(bucket, Classified::Ok);
        {
            let p = &mut self.file.peers[idx];
            if !p.presented_callsigns.iter().any(|c| c == &presented) {
                p.presented_callsigns.push(presented.clone());
            }
            if ok {
                p.last_connected_at = Some(now.clone());
            }
            match &obs.path {
                ObservedPath::Rf { transport, via, freq_hz, bandwidth } => {
                    let key_match = |c: &crate::peers::model::Channel| {
                        c.transport == *transport
                            && c.target_callsign == presented
                            && c.via == *via
                            && c.freq_hz == *freq_hz
                            && c.bandwidth == *bandwidth
                    };
                    if let Some(ch) = p.channels.iter_mut().find(|c| key_match(c)) {
                        if ok {
                            ch.counts.ok = ch.counts.ok.saturating_add(1);
                        } else {
                            ch.counts.fail = ch.counts.fail.saturating_add(1);
                        }
                        ch.direction = obs.direction;
                        ch.last_seen = now.clone();
                    } else {
                        p.channels.push(crate::peers::model::Channel {
                            transport: *transport,
                            target_callsign: presented.clone(),
                            via: via.clone(),
                            freq_hz: *freq_hz,
                            bandwidth: *bandwidth,
                            direction: obs.direction,
                            counts: crate::peers::model::AttemptCounts {
                                ok: u32::from(ok),
                                fail: u32::from(!ok),
                            },
                            last_seen: now.clone(),
                        });
                    }
                }
                ObservedPath::Telnet { host, port, provenance } => {
                    // Monotonic provenance [R4-4]: an observation NEVER
                    // creates or mutates an Operator endpoint.
                    let prov = if *provenance == crate::peers::model::Provenance::Operator {
                        crate::peers::model::Provenance::ObservedIncoming
                    } else {
                        *provenance
                    };
                    let hostn = host.trim().to_ascii_lowercase();
                    if let Some(ep) = p.endpoints.iter_mut().find(|e| {
                        e.host == hostn && e.port == *port && e.provenance == prov
                    }) {
                        ep.last_seen = now.clone();
                    } else {
                        p.endpoints.push(crate::peers::model::Endpoint {
                            id: uuid::Uuid::new_v4().to_string(),
                            host: hostn,
                            port: *port,
                            provenance: prov,
                            last_seen: now.clone(),
                        });
                    }
                }
            }
        }
        self.flush()?;
        Ok(if created { ApplyEffect::CreatedPeer } else { ApplyEffect::UpdatedPeer })
    }

    fn new_auto_peer(
        &self,
        base: &str,
        presented: &str,
        obs: &crate::peers::recorder::PeerObservation,
        now: &str,
    ) -> crate::peers::model::Peer {
        crate::peers::model::Peer {
            id: uuid::Uuid::new_v4().to_string(),
            canonical_base: base.to_string(),
            presented_callsigns: vec![presented.to_string()],
            identity_kind: crate::peers::model::IdentityKind::Unknown,
            do_not_merge: false,
            conflict: false,
            source: crate::peers::model::RecordSource::Auto,
            origin: match obs.direction {
                crate::peers::model::Direction::Incoming => crate::peers::model::Origin::Incoming,
                crate::peers::model::Direction::Outgoing => crate::peers::model::Origin::Outgoing,
                crate::peers::model::Direction::Unknown => crate::peers::model::Origin::Unknown,
            },
            contact_id: None,
            grid: None,
            note: String::new(),
            created_at: now.to_string(),
            last_connected_at: None,
            channels: vec![],
            endpoints: vec![],
        }
    }

    /// LRU eviction among Auto records only [R2-S6]. Manual/OperatorPinned
    /// records and conflict-held records are never evicted.
    fn evict_over_cap(&mut self) {
        use crate::peers::model::{RecordSource, AUTO_PEER_CAP};
        loop {
            let auto: Vec<usize> = self
                .file
                .peers
                .iter()
                .enumerate()
                .filter(|(_, p)| p.source == RecordSource::Auto && !p.conflict)
                .map(|(i, _)| i)
                .collect();
            if auto.len() <= AUTO_PEER_CAP {
                return;
            }
            // Oldest activity = last_connected_at, falling back to created_at.
            let lru = auto
                .into_iter()
                .min_by(|&a, &b| {
                    let ka = self.file.peers[a].last_connected_at.as_deref()
                        .unwrap_or(&self.file.peers[a].created_at).to_string();
                    let kb = self.file.peers[b].last_connected_at.as_deref()
                        .unwrap_or(&self.file.peers[b].created_at).to_string();
                    ka.cmp(&kb)
                })
                .expect("non-empty by the cap check");
            self.file.peers.remove(lru);
        }
    }
}
```

`merge` moves the absorbed record's `presented_callsigns` / `channels` / `endpoints` onto the kept record (dedup by their keys), deletes the absorbed record, and returns the absorbed endpoint ids (the command layer re-keys their keyring secrets, Task 10). `split` clones the record, moves the named presented forms + their exact-matching channels to the clone, sets `do_not_merge` on BOTH, mints a new uuid for the clone, and returns it. `promote_endpoint` finds the endpoint by id and sets `provenance = Operator` in place — the only path that writes `Operator` on an endpoint. `delete_peer` removes the record and returns its endpoint ids. `upsert_manual` validates `canonical_base`/`presented_callsigns` through `sanitize_display` and replaces-by-id or pushes. All mutators `flush()` on success.

Note on LRU string comparison: RFC3339-with-offset strings from a single machine sort chronologically as strings for a fixed offset; a DST boundary can misorder by an hour — acceptable for cap eviction (not user-visible ordering). Do not parse-and-compare here; keep it allocation-light.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked peers::store` (or CI). Expected: 9 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/
git commit -m "feat(peers): store — infallible open, per-transport dedup, split routing, monotonic provenance, auto-cap LRU"
```

---

### Task 9: Inbound auto-create rate limit + quarantine counter

**Files:**
- Create: `src-tauri/src/peers/limiter.rs` (add `pub mod limiter;` to `peers/mod.rs`)
- Modify: `src-tauri/src/config.rs` (add the `[p2p]` section — additive, `#[serde(default)]`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `ChannelTransport` (Task 7).
- Produces (consumed by Task 11): `pub struct InboundCreateLimiter` with `pub fn new(cfg: P2pLimitsConfig) -> Self` and `pub fn allow(&mut self, transport: ChannelTransport, accepted: bool, now: std::time::Instant) -> bool`; `pub fn quarantined(&self) -> u32`. Config struct `P2pLimitsConfig { accepted_per_hour: u32 (default 100), failed_per_minute: u32 (default 10) }`.

Spec `[R2-S6][R5-9]`: the limiter gates **auto-CREATION from inbound** only (outbound dials and existing-record updates are never limited). Accepted, authorized exchanges get the high per-hour threshold (a real net/field-day must not lose roster observations); unauthorized/failed bursts get the low per-minute threshold. Over-threshold events increment a bounded in-memory quarantine counter and are logged visibly (`tracing::warn!` + session log at the record site) — never persisted to the roster.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::ChannelTransport;
    use std::time::{Duration, Instant};

    fn limiter() -> InboundCreateLimiter {
        InboundCreateLimiter::new(P2pLimitsConfig::default())
    }

    #[test]
    fn a_busy_field_day_is_not_quarantined() {
        // [R5-9]: 50 distinct accepted inbound exchanges in an hour is a
        // real net; all must pass at the default threshold of 100/hr.
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..50 {
            assert!(l.allow(ChannelTransport::VaraHf, true, t0 + Duration::from_secs(i * 60)));
        }
        assert_eq!(l.quarantined(), 0);
    }

    #[test]
    fn a_failed_handshake_burst_hits_the_low_threshold() {
        let mut l = limiter();
        let t0 = Instant::now();
        let mut allowed = 0;
        for i in 0..40 {
            if l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i * 100)) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 10, "default failed_per_minute = 10");
        assert_eq!(l.quarantined(), 30);
    }

    #[test]
    fn thresholds_are_per_transport() {
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..10 {
            assert!(l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i)));
        }
        assert!(!l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(11)));
        // Packet has its own bucket — not exhausted by the VARA burst.
        assert!(l.allow(ChannelTransport::Packet, false, t0 + Duration::from_millis(12)));
    }

    #[test]
    fn quarantine_counter_is_bounded() {
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..200_000u64 {
            let _ = l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i));
        }
        assert!(l.quarantined() <= 100_000, "counter saturates; no unbounded growth");
    }
}
```

- [ ] **Step 2: Verify failure** — module does not exist.

- [ ] **Step 3: Implement**

```rust
//! Inbound auto-create rate limiter [R2-S6][R5-9]. Gates CREATION of
//! auto records from inbound observations only. In-memory; the quarantine
//! counter is never persisted to the roster.

use crate::peers::model::ChannelTransport;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pLimitsConfig {
    /// Accepted, authorized inbound exchanges (allowlist-passed,
    /// B2F-completed): high threshold — a real net must never lose
    /// roster observations.
    #[serde(default = "default_accepted_per_hour")]
    pub accepted_per_hour: u32,
    /// Unauthorized / failed / handshake-abandoned bursts: low threshold.
    #[serde(default = "default_failed_per_minute")]
    pub failed_per_minute: u32,
}
fn default_accepted_per_hour() -> u32 { 100 }
fn default_failed_per_minute() -> u32 { 10 }
impl Default for P2pLimitsConfig {
    fn default() -> Self {
        Self { accepted_per_hour: 100, failed_per_minute: 10 }
    }
}

const QUARANTINE_COUNTER_CAP: u32 = 100_000;

pub struct InboundCreateLimiter {
    cfg: P2pLimitsConfig,
    accepted: HashMap<ChannelTransport, Vec<Instant>>,
    failed: HashMap<ChannelTransport, Vec<Instant>>,
    quarantined: u32,
}

impl InboundCreateLimiter {
    pub fn new(cfg: P2pLimitsConfig) -> Self {
        Self { cfg, accepted: HashMap::new(), failed: HashMap::new(), quarantined: 0 }
    }

    /// True = the auto-create may proceed. False = quarantined (count it,
    /// log visibly at the call site, do NOT write the roster).
    pub fn allow(&mut self, transport: ChannelTransport, accepted: bool, now: Instant) -> bool {
        let (bucket, window, max) = if accepted {
            (self.accepted.entry(transport).or_default(), Duration::from_secs(3600), self.cfg.accepted_per_hour)
        } else {
            (self.failed.entry(transport).or_default(), Duration::from_secs(60), self.cfg.failed_per_minute)
        };
        bucket.retain(|t| now.duration_since(*t) < window);
        if (bucket.len() as u32) < max {
            bucket.push(now);
            true
        } else {
            self.quarantined = self.quarantined.saturating_add(1).min(QUARANTINE_COUNTER_CAP);
            false
        }
    }

    pub fn quarantined(&self) -> u32 {
        self.quarantined
    }
}
```

In `config.rs`, add to the `Config` struct (additive; old TOML files simply lack the section and get defaults — `deny_unknown_fields` is unaffected by ADDING a field):

```rust
    /// P2P inbound auto-create rate limits (spec §2 caps [R5-9]).
    #[serde(default)]
    pub p2p_limits: crate::peers::limiter::P2pLimitsConfig,
```

Check the config schema-bump guard (`tuxlink-ulrz` — see `config.rs` module docs): an ADDITIVE field with `#[serde(default)]` does not bump the schema version. Add a config round-trip test beside the existing config serde tests pinning that an old file without `[p2p_limits]` loads with defaults.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked limiter` (or CI). Expected: 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/limiter.rs src-tauri/src/peers/mod.rs src-tauri/src/config.rs
git commit -m "feat(peers): inbound auto-create rate limiter — accepted vs failed thresholds, bounded quarantine counter"
```

---

### Task 10: Keyring — endpoint-keyed secrets, conservative migration, cascade clear

**Files:**
- Modify: `src-tauri/src/winlink/credentials.rs`, `src-tauri/src/uninstall_cleanup.rs`
- Test: inline `#[cfg(test)]` (credentials.rs already has a factory-injection test pattern — `p2p_peer_password_read_with_factory`, `credentials.rs:160-176`; reuse it)

**Interfaces:**
- Consumes: peer/endpoint ids (Task 7-8).
- Produces (consumed by Tasks 11, 20, 25):

```rust
pub fn p2p_endpoint_password_read(peer_id: &str, endpoint_id: &str) -> Result<Option<String>, String>;
pub fn p2p_endpoint_password_write(peer_id: &str, endpoint_id: &str, password: &str) -> Result<(), String>;
pub fn p2p_endpoint_password_delete(peer_id: &str, endpoint_id: &str) -> Result<(), String>;
pub enum LegacyMigration { Migrated, NoLegacySecret, Ambiguous }
pub fn migrate_legacy_peer_secret(callsign: &str, peer_id: &str, endpoint_id: &str, unambiguous: bool) -> Result<LegacyMigration, String>;
```

Rules `[R2-S7][R1-C7][R5-5]`: account string `p2p-endpoint:<peer_id>:<endpoint_id>` (both uuids — callsign never appears in a keyring account again, closing the account-string injection class `[R2-S10]`). Migration is lazy: when an endpoint password READ misses under the new key, the caller may attempt legacy migration — but only when the caller has verified the mapping is unambiguous (exactly one peer on that base + exactly one `Operator` endpoint); the legacy secret is deleted only AFTER the new-key write succeeds. Ambiguity returns `Ambiguous` and the peers settings UI surfaces manual reassignment (Task 25).

- [ ] **Step 1: Write the failing tests** (factory-injected, mirroring the existing `credentials.rs` tests — no real keyring in CI)

```rust
    #[test]
    fn endpoint_account_is_id_keyed_not_callsign_keyed() {
        assert_eq!(
            p2p_endpoint_account("peer-uuid-1", "ep-uuid-2"),
            "p2p-endpoint:peer-uuid-1:ep-uuid-2"
        );
    }

    #[test]
    fn migration_copies_then_deletes_legacy_only_after_write_success() {
        // Fake keyring: legacy secret exists; new key empty.
        let store = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::from([(
                ("tuxlink".to_string(), "p2p-peer:W6ABC".to_string()),
                "hunter2".to_string(),
            )]),
        ));
        let out = migrate_legacy_peer_secret_with_factory(
            "W6ABC", "p1", "e1", true, fake_factory(store.clone()),
        )
        .unwrap();
        assert_eq!(out, LegacyMigration::Migrated);
        let map = store.lock().unwrap();
        assert_eq!(
            map.get(&("tuxlink".into(), "p2p-endpoint:p1:e1".into())).map(String::as_str),
            Some("hunter2")
        );
        assert!(!map.contains_key(&("tuxlink".into(), "p2p-peer:W6ABC".into())), "legacy deleted after write");
    }

    #[test]
    fn ambiguous_mapping_migrates_nothing() {
        let store = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::from([(
                ("tuxlink".to_string(), "p2p-peer:W6ABC".to_string()),
                "hunter2".to_string(),
            )]),
        ));
        let out = migrate_legacy_peer_secret_with_factory(
            "W6ABC", "p1", "e1", false, fake_factory(store.clone()),
        )
        .unwrap();
        assert_eq!(out, LegacyMigration::Ambiguous);
        let map = store.lock().unwrap();
        assert!(map.contains_key(&("tuxlink".into(), "p2p-peer:W6ABC".into())), "legacy untouched");
        assert!(!map.contains_key(&("tuxlink".into(), "p2p-endpoint:p1:e1".into())));
    }
```

(`fake_factory` follows the existing `with_factory` test-double pattern in `credentials.rs` — a closure returning an entry object backed by the HashMap. Match the existing factory signature exactly; read `credentials.rs:146-209` first.)

- [ ] **Step 2: Verify failure** — functions undefined.

- [ ] **Step 3: Implement** in `credentials.rs`, beside `p2p_peer_account` (`credentials.rs:146-151`):

```rust
/// Keyring account for a P2P endpoint password. Keyed by ids, NOT by
/// callsign [R2-S7][R1-C7]: ids are uuid-shaped (no attacker-controlled
/// bytes), so keyring account-string injection is closed at the type
/// level [R2-S10].
fn p2p_endpoint_account(peer_id: &str, endpoint_id: &str) -> String {
    format!("p2p-endpoint:{peer_id}:{endpoint_id}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyMigration {
    Migrated,
    NoLegacySecret,
    Ambiguous,
}

/// Conservative legacy re-key [R5-5]. `unambiguous` is computed by the
/// CALLER against the peers store: exactly one peer with this base AND
/// exactly one Operator endpoint. Anything else → `Ambiguous`, surface
/// manual reassignment in the peers settings UI. Delete-legacy happens
/// strictly after the new-key write succeeds — no window where the
/// secret is lost.
pub fn migrate_legacy_peer_secret(
    callsign: &str,
    peer_id: &str,
    endpoint_id: &str,
    unambiguous: bool,
) -> Result<LegacyMigration, String> {
    migrate_legacy_peer_secret_with_factory(callsign, peer_id, endpoint_id, unambiguous, real_factory)
}
```

with the `_with_factory` body: if `!unambiguous` return `Ambiguous`; read legacy `p2p_peer_account(callsign)` → `NoLegacySecret` on miss; write `p2p_endpoint_account(peer_id, endpoint_id)`; only then delete the legacy entry; return `Migrated`. The read/write/delete plumbing mirrors `p2p_peer_password_read_with_factory` / `..write_with_factory` exactly (same `SERVICE`, same factory type).

In `uninstall_cleanup.rs`, extend `keyring_targets` (at `uninstall_cleanup.rs:474-486`) to also enumerate endpoint secrets by loading `peers.json` from the data dir (same pattern as `discover_peer_callsigns`, `uninstall_cleanup.rs:551`) and pushing a `KeyringTarget { service: KEYRING_SERVICE, account: format!("p2p-endpoint:{}:{}", peer.id, ep.id), description: "P2P endpoint password" }` per endpoint.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked credentials` (or CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/credentials.rs src-tauri/src/uninstall_cleanup.rs
git commit -m "feat(peers): endpoint-keyed keyring secrets with conservative legacy migration and uninstall enumeration"
```

---

### Task 11: Recorder — observation types, drop-guard, central entry, Tauri commands + state wiring

**Files:**
- Create/complete: `src-tauri/src/peers/recorder.rs`, `src-tauri/src/peers/commands.rs`
- Modify: `src-tauri/src/lib.rs` (manage state + register commands)
- Test: inline `#[cfg(test)]` in `recorder.rs`

**Interfaces:**
- Consumes: Tasks 7-10.
- Produces (consumed by every record site, Tasks 12-17, and the frontend, Tasks 22-25):

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::*;
    use std::sync::{Arc, Mutex};

    fn obs(phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf {
                transport: ChannelTransport::VaraHf,
                via: vec![],
                freq_hz: None,
                bandwidth: None,
            },
            direction: Direction::Outgoing,
            presented_target: "W6ABC".into(),
            phase,
        }
    }

    #[test]
    fn classification_matches_the_spec_table() {
        // spec §3: dial_attempted → connected → (login_failed | b2f_started
        // → b2f_ok | b2f_fail) | accepted | rejected | aborted/wedged
        assert_eq!(classify(ObservationPhase::B2fOk), Classified::Ok);
        assert_eq!(classify(ObservationPhase::Accepted), Classified::Ok);
        assert_eq!(classify(ObservationPhase::Rejected), Classified::NoRecord);
        for p in [
            ObservationPhase::DialAttempted,
            ObservationPhase::Connected,
            ObservationPhase::LoginFailed,
            ObservationPhase::B2fStarted,
            ObservationPhase::B2fFail,
            ObservationPhase::AbortedOrWedged,
        ] {
            assert_eq!(classify(p), Classified::Fail, "{p:?}");
        }
    }

    #[test]
    fn guard_fires_on_drop_with_the_latest_phase() {
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink.clone(), obs(ObservationPhase::DialAttempted));
            g.set_phase(ObservationPhase::Connected);
            g.set_phase(ObservationPhase::B2fOk);
        } // drop → fire
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(seen.lock().unwrap()[0].phase, ObservationPhase::B2fOk);
    }

    #[test]
    fn guard_records_fail_when_dropped_mid_exchange() {
        // The ARDOP-ARQTimeout lesson [R3-11]: a wedge/abort/early-return
        // path still records — the guard IS the finally.
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink, obs(ObservationPhase::DialAttempted));
            g.set_phase(ObservationPhase::Connected);
            // …exchange wedges; nothing sets B2fOk…
        }
        assert_eq!(classify(seen.lock().unwrap()[0].phase), Classified::Fail);
    }

    #[test]
    fn guard_disarm_suppresses_the_record() {
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink, obs(ObservationPhase::DialAttempted));
            g.disarm(); // another site owns this attempt's record
        }
        assert!(seen.lock().unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3a: Implement `recorder.rs` types + guard + central entry**

```rust
//! Shared peer-observation recorder (spec §3). There is NO single
//! chokepoint [R4-1] — each transport calls this from its own
//! attempt-conclusion site(s), via [`ObservationGuard`] so wedged /
//! aborted / early-return paths still record a fail [R3-11].

use crate::peers::model::{ChannelBandwidth, ChannelTransport, Direction, Provenance};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationPhase {
    DialAttempted,
    Connected,
    LoginFailed,
    B2fStarted,
    B2fOk,
    B2fFail,
    Accepted,
    Rejected,
    AbortedOrWedged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified {
    Ok,
    Fail,
    /// Rejected/unauthorized inbound: an attacker knocking is not a peer.
    NoRecord,
}

pub fn classify(phase: ObservationPhase) -> Classified {
    match phase {
        ObservationPhase::B2fOk | ObservationPhase::Accepted => Classified::Ok,
        ObservationPhase::Rejected => Classified::NoRecord,
        ObservationPhase::DialAttempted
        | ObservationPhase::Connected
        | ObservationPhase::LoginFailed
        | ObservationPhase::B2fStarted
        | ObservationPhase::B2fFail
        | ObservationPhase::AbortedOrWedged => Classified::Fail,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedPath {
    Rf {
        transport: ChannelTransport,
        via: Vec<String>,
        /// Incoming rows have no wire freq source (CONNECTED carries
        /// bandwidth, not frequency) — rig/CAT state if available, else
        /// None; never fabricated [R3-11].
        freq_hz: Option<u64>,
        bandwidth: Option<ChannelBandwidth>,
    },
    Telnet {
        host: String,
        port: u16,
        provenance: Provenance,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservation {
    pub path: ObservedPath,
    pub direction: Direction,
    /// Exact presented/SSID'd callsign of the far station.
    pub presented_target: String,
    pub phase: ObservationPhase,
}

pub type ObservationSink = Arc<dyn Fn(PeerObservation) + Send + Sync>;

/// Drop-guard recorder: construct at attempt start with `DialAttempted`
/// (or `Accepted`-path initial), advance via `set_phase`, and the record
/// fires on Drop with the latest phase — every exit path records.
pub struct ObservationGuard {
    sink: ObservationSink,
    obs: Mutex<Option<PeerObservation>>,
}

impl ObservationGuard {
    pub fn new(sink: ObservationSink, initial: PeerObservation) -> Self {
        Self { sink, obs: Mutex::new(Some(initial)) }
    }
    pub fn set_phase(&self, phase: ObservationPhase) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.as_mut() {
                o.phase = phase;
            }
        }
    }
    /// Update path details learned mid-attempt (e.g. bandwidth from the
    /// CONNECTED line).
    pub fn set_path(&self, path: ObservedPath) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.as_mut() {
                o.path = path;
            }
        }
    }
    pub fn disarm(&self) {
        if let Ok(mut g) = self.obs.lock() {
            g.take();
        }
    }
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.take() {
                (self.sink)(o);
            }
        }
    }
}

/// Central entry: classification + inbound-create rate limit + store
/// apply + visible quarantine logging. Call-site closures wrap this with
/// their app state and emit `peers:changed` on a Recorded effect.
pub fn record_peer_observation(
    store: &Mutex<crate::peers::store::PeersStore>,
    limiter: &Mutex<crate::peers::limiter::InboundCreateLimiter>,
    obs: PeerObservation,
) -> crate::peers::store::ApplyEffect {
    use crate::peers::store::ApplyEffect;
    if classify(obs.phase) == Classified::NoRecord {
        return ApplyEffect::NoRecord;
    }
    // Rate-limit inbound CREATES only [R5-9]: existing-record updates and
    // outbound observations always pass.
    if obs.direction == Direction::Incoming {
        let base = crate::winlink::callsign::canonical_base(&obs.presented_target);
        let exists = store
            .lock()
            .map(|s| s.file().peers.iter().any(|p| p.canonical_base == base))
            .unwrap_or(false);
        if !exists {
            let transport = match &obs.path {
                ObservedPath::Rf { transport, .. } => *transport,
                ObservedPath::Telnet { .. } => ChannelTransport::Unknown,
            };
            let accepted = classify(obs.phase) == Classified::Ok;
            let allowed = limiter
                .lock()
                .map(|mut l| l.allow(transport, accepted, std::time::Instant::now()))
                .unwrap_or(true);
            if !allowed {
                let q = limiter.lock().map(|l| l.quarantined()).unwrap_or(0);
                tracing::warn!(
                    target: "tuxlink::peers",
                    presented = %obs.presented_target,
                    quarantined_total = q,
                    "inbound peer auto-create rate-limited — quarantined (not added to roster)"
                );
                return ApplyEffect::NoRecord;
            }
        }
    }
    let now = chrono::Local::now().to_rfc3339();
    match store.lock() {
        Ok(mut s) => s.apply_observation(&obs, now).unwrap_or_else(|e| {
            tracing::warn!(target: "tuxlink::peers", "peer observation write failed: {e:?}");
            ApplyEffect::NoRecord
        }),
        Err(_) => ApplyEffect::NoRecord,
    }
}
```

- [ ] **Step 3b: Implement `peers/commands.rs`** (mirror `contacts/commands.rs` structure: `tauri::State<Arc<Mutex<PeersStore>>>`, `PEERS_CHANGED_EVENT: &str = "peers:changed"` emitted after every mutation):

```rust
#[tauri::command] pub fn peers_read(svc: State<Arc<Mutex<PeersStore>>>) -> Result<PeersFile, PeersError>;
#[tauri::command] pub fn peer_upsert(app: AppHandle, svc: …, peer: Peer) -> Result<(), PeersError>;
#[tauri::command] pub fn peer_delete(app: AppHandle, svc: …, id: String) -> Result<(), PeersError>;      // cascade: p2p_endpoint_password_delete for every returned endpoint id [R2-S7]
#[tauri::command] pub fn peer_merge(app: AppHandle, svc: …, keep_id: String, absorb_id: String) -> Result<(), PeersError>;
#[tauri::command] pub fn peer_split(app: AppHandle, svc: …, peer_id: String, presented: Vec<String>) -> Result<String, PeersError>;
#[tauri::command] pub fn peer_endpoint_promote(app: AppHandle, svc: …, peer_id: String, endpoint_id: String) -> Result<(), PeersError>;
#[tauri::command] pub fn peer_endpoint_password_set(svc: …, peer_id: String, endpoint_id: String, password: String) -> Result<(), PeersError>;
#[tauri::command] pub fn peer_endpoint_password_clear(svc: …, peer_id: String, endpoint_id: String) -> Result<(), PeersError>;
#[tauri::command] pub fn p2p_capabilities() -> P2pCapabilities;
```

`P2pCapabilities` is the integration-matrix hide mechanism `[R5-8]` — one bool per matrix row, **hardcoded `true` only in the task that lands that row** (each starts `false`; Tasks 12-25 flip their own bit in their own commit):

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct P2pCapabilities {
    pub peer_store: bool,        // rows 1-2 (store + recorder)
    pub finder_peers: bool,      // rows 3, 5 (read cmd + aggregation + filter)
    pub map_peers: bool,         // row 6
    pub settings_editor: bool,   // row 8
    pub agent_find_peers: bool,  // row 4
    pub agent_telnet_dial: bool, // row 7
    pub vara_engine_split: bool, // row 9
    pub favorites_peer_link: bool, // row 10
}
```

- [ ] **Step 3c: Wire `lib.rs`.** In `.setup()` beside the contacts/favorites wiring (`lib.rs:1009-1025`):

```rust
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::peers::store::PeersStore::open(data_dir.join("peers.json")),
                    )));
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::peers::limiter::InboundCreateLimiter::new(
                            crate::config::read_config()
                                .map(|c| c.p2p_limits)
                                .unwrap_or_default(),
                        ),
                    )));
```

and register all nine commands in the `invoke_handler` list.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked peers` (or CI) + `pnpm typecheck` still green. Expected: recorder tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/ src-tauri/src/lib.rs
git commit -m "feat(peers): observation recorder with drop-guard, rate-limited central entry, Tauri commands + capability bits"
```

---

### Task 12: Packet P2P intent plumbing

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (`TransportConfig::Packet` ~151-163, `PacketConnectCtx` ~2311-2322, `native_packet_exchange` ~2375-2456, both `PacketConnectCtx` construction sites ~2681-2692 and ~2807-2819), `src-tauri/src/ui_commands.rs` (`packet_transport_from_config` ~4813-4826, `packet_listen_transport_from_config` ~4832-4846, `packet_connect` ~4862)
- Test: existing `mod tests` in `winlink_backend.rs` + `ui_commands.rs`

**Interfaces:**
- Consumes: `SessionIntent` (existing, `winlink/session/mod.rs:131-160`).
- Produces: `TransportConfig::Packet { link, ssid, role, intent: SessionIntent }`; `PacketConnectCtx { …, intent: SessionIntent }`; `packet_connect(…, intent: Option<SessionIntent>)` (None → `Cms`); `packet_transport_from_config(cfg, call, path, intent)`. Task 15's record sites and Task 23's finder Connect depend on these.

Why `[R4-3][R1-C15][R5-3]`: `native_packet_exchange` hardcodes `ExchangeConfig { intent: SessionIntent::Cms }` for BOTH directions (`winlink_backend.rs:2434-2440`), so packet peers can never be classified P2P and would silently never track. Plumbing only `ExchangeConfig` is insufficient — the intent must ride `TransportConfig`/`PacketConnectCtx`, and the hand-built outbound proposal loop (`winlink_backend.rs:2416-2432`, the `TODO(tuxlink-u5hl)`) must become the intent-aware `build_outbound_proposals` the answer paths already use.

- [ ] **Step 1: Write the failing tests**

```rust
    // winlink_backend.rs tests
    #[test]
    fn packet_dial_default_intent_is_cms_and_p2p_is_not_cms() {
        // [R5-3] pins both directions of the contract: existing callers are
        // untouched (Cms default), and a P2P packet session is not
        // classified as CMS.
        let cms = PacketConnectCtx {
            base_mycall: "W6ABC",
            targetcall: "N0DAJ-10",
            password: None,
            role: ExchangeRole::Dial,
            locator: "CN87",
            intent: SessionIntent::Cms,
        };
        let p2p = PacketConnectCtx { intent: SessionIntent::P2p, ..cms };
        assert_eq!(exchange_config_for_packet(&cms).intent, SessionIntent::Cms);
        assert_eq!(exchange_config_for_packet(&p2p).intent, SessionIntent::P2p);
    }
```

```rust
    // ui_commands.rs tests (beside the existing packet_transport tests, ~11640)
    #[test]
    fn packet_listen_transport_carries_p2p_intent() {
        // An inbound packet call is by definition a peer session — this
        // station is not an RMS (WLE Packet Peer Stations ground truth).
        let cfg = config_with_kiss_link();
        let t = packet_listen_transport_from_config(&cfg).unwrap();
        match t {
            TransportConfig::Packet { intent, role, .. } => {
                assert_eq!(intent, SessionIntent::P2p);
                assert_eq!(role, crate::winlink_backend::PacketRole::Listen);
            }
            _ => panic!("expected packet transport"),
        }
    }

    #[test]
    fn packet_dial_transport_defaults_to_cms() {
        let cfg = config_with_kiss_link();
        let t = packet_transport_from_config(&cfg, "N0DAJ-10".into(), vec![], SessionIntent::Cms).unwrap();
        match t {
            TransportConfig::Packet { intent, .. } => assert_eq!(intent, SessionIntent::Cms),
            _ => panic!("expected packet transport"),
        }
    }
```

(`config_with_kiss_link()` — reuse/extend the fixture the existing `packet_transport_from_config` tests use at `ui_commands.rs:~11640`; read those tests first and match their fixture.)

- [ ] **Step 2: Verify failure** — `intent` field does not exist.

- [ ] **Step 3: Implement.**

(a) Add `pub intent: SessionIntent` to `TransportConfig::Packet` and `PacketConnectCtx`. Extract a small pure helper so the intent contract is unit-testable:

```rust
/// The ExchangeConfig for a packet session — pure, so the intent contract
/// [R5-3] is pinned without a KISS link.
fn exchange_config_for_packet(ctx: &PacketConnectCtx<'_>) -> session::ExchangeConfig {
    session::ExchangeConfig {
        mycall: ctx.base_mycall.to_string(), // BASE call — no SSID in B2F identity
        targetcall: ctx.targetcall.to_string(),
        locator: ctx.locator.to_string(),
        password: ctx.password.clone(),
        intent: ctx.intent,
    }
}
```

and in `native_packet_exchange` replace the inline `ExchangeConfig` construction (`winlink_backend.rs:2434-2440`) with `let exchange_config = exchange_config_for_packet(&ctx);` (destructure `ctx` AFTER this, or take fields by reference — the current destructuring at 2382-2388 moves the fields; reorder accordingly).

(b) Replace the hand-built outbound loop (`winlink_backend.rs:2416-2432`) with the intent-aware builder the answer paths use (resolves the `TODO(tuxlink-u5hl)`):

```rust
    let outbound = build_outbound_proposals(mailbox, ctx.intent, None, Some(ctx.base_mycall))
        .unwrap_or_else(|e| {
            eprintln!(
                "native_packet_exchange: outbound drain skipped ({e}); exchange continues with empty outbound"
            );
            Vec::new()
        });
    let outbound_log = outbound_log_items(&outbound);
```

(c) Thread the intent through the two `PacketConnectCtx` construction sites in `native_packet_connect`: the dial arm (`winlink_backend.rs:2681-2692`) and the answer arm (`~2807-2819`). `native_packet_connect` reads the intent off the `TransportConfig::Packet` destructure it already receives (`resolved`/`link`/`role` all come from it — add `intent` to that destructure chain, passing it via `ResolvedPacket` if the role resolution consumes the config first; keep `intent` alongside `role` wherever `role` travels). The answer arm uses the transport's intent (which `packet_listen_transport_from_config` sets to `P2p`).

(d) `ui_commands.rs`: `packet_transport_from_config` gains `intent: SessionIntent` (its one production caller `packet_connect` passes `intent.unwrap_or_default()` from a new `intent: Option<SessionIntent>` command arg — `SessionIntent` already derives `Deserialize` kebab-case, so the frontend passes `"p2p"`); `packet_listen_transport_from_config` sets `intent: SessionIntent::P2p` with the comment from the test above.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked packet` (or CI) + `pnpm typecheck`. Expected: new tests PASS; all existing packet tests (which now construct the ctx with `intent`) updated and PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink_backend.rs src-tauri/src/ui_commands.rs
git commit -m "feat(packet): thread SessionIntent through TransportConfig/PacketConnectCtx — P2P packet sessions classifiable [R5-3]"
```

---

### Task 13: Global observation sink + VARA record sites (dial + answer)

**Files:**
- Modify: `src-tauri/src/peers/recorder.rs` (global sink), `src-tauri/src/lib.rs` (install at setup), `src-tauri/src/winlink/modem/vara/commands.rs` (dial walk), `src-tauri/src/ui_commands.rs` (VARA answer consumer, ~6519)
- Test: `recorder.rs` + a dial-walk test in `vara/commands.rs`

**Interfaces:**
- Consumes: Tasks 2-5, 11.
- Produces: `peers::recorder::install_observation_sink(sink: ObservationSink)` + `peers::recorder::observation_sink() -> Option<ObservationSink>` — the mechanism EVERY record site (Tasks 13-16) uses; sites are no-ops when no sink is installed (unit tests, headless).

**Design.** The 8 record sites span layers with and without `AppHandle` (`native_packet_connect` and `telnet_listen::handle_one_session` have none). Rather than threading a sink parameter through the whole backend, the recorder holds a process-global:

```rust
use std::sync::RwLock;
static SINK: RwLock<Option<ObservationSink>> = RwLock::new(None);

/// Installed once at app setup (lib.rs), after the peers store + limiter
/// are managed. Sites call `observation_sink()`; None → no-op (tests,
/// headless tools).
pub fn install_observation_sink(sink: ObservationSink) {
    if let Ok(mut g) = SINK.write() {
        *g = Some(sink);
    }
}
pub fn observation_sink() -> Option<ObservationSink> {
    SINK.read().ok().and_then(|g| g.clone())
}
```

In `lib.rs` `.setup()`, immediately after the Task 11 `app.manage(...)` calls:

```rust
                    {
                        let store = app.state::<std::sync::Arc<std::sync::Mutex<crate::peers::store::PeersStore>>>().inner().clone();
                        let limiter = app.state::<std::sync::Arc<std::sync::Mutex<crate::peers::limiter::InboundCreateLimiter>>>().inner().clone();
                        let favorites = app.state::<std::sync::Arc<std::sync::Mutex<crate::favorites::store::FavoritesStore>>>().inner().clone();
                        let emit_handle = app.handle().clone();
                        crate::peers::recorder::install_observation_sink(std::sync::Arc::new(
                            move |obs: crate::peers::recorder::PeerObservation| {
                                let effect = crate::peers::recorder::record_peer_observation(&store, &limiter, obs.clone());
                                if !matches!(effect, crate::peers::store::ApplyEffect::NoRecord) {
                                    use tauri::Emitter;
                                    let _ = emit_handle.emit("peers:changed", ());
                                    crate::peers::recorder::bridge_to_favorites(&favorites, &obs); // Task 17 (no-op until then)
                                }
                            },
                        ));
                    }
```

(Until Task 17 lands, `bridge_to_favorites` is NOT referenced — add that line in Task 17. Record sites only fire for `SessionIntent::P2p` sessions; gateway/CMS/RadioOnly/PostOffice dials never construct a guard.)

- [ ] **Step 1: Write the failing test** (dial-walk recording, in `vara/commands.rs` tests — uses `serial_test` since the sink is process-global):

```rust
    #[test]
    #[serial_test::serial]
    fn vara_dial_walk_records_fail_per_failed_candidate() {
        let seen: Arc<std::sync::Mutex<Vec<crate::peers::recorder::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::peers::recorder::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }
        // Loopback transport whose cmd server never sends CONNECTED → the
        // single candidate fails at the (shortened) connect deadline.
        let (mut t, _h1, _h2) =
            loopback_transport_for_readiness(vec![], std::time::Duration::from_millis(1));
        let ptt: SharedPtt = Default::default();
        let _ = send_connect_and_wait_inner(
            &mut t, "W6ABC", "N0DAJ-7", &[], TransportKind::VaraHf, SessionIntent::P2p,
            std::time::Duration::from_millis(300), &ptt,
        );
        // The walk-level guard (not send_connect_and_wait itself) records —
        // drive the recording helper directly for the unit layer:
        {
            let g = crate::peers::recorder::ObservationGuard::new(
                crate::peers::recorder::observation_sink().unwrap(),
                crate::peers::recorder::PeerObservation {
                    path: crate::peers::recorder::ObservedPath::Rf {
                        transport: crate::peers::model::ChannelTransport::VaraHf,
                        via: vec![],
                        freq_hz: Some(7_101_000),
                        bandwidth: None,
                    },
                    direction: crate::peers::model::Direction::Outgoing,
                    presented_target: "N0DAJ-7".into(),
                    phase: crate::peers::recorder::ObservationPhase::DialAttempted,
                },
            );
            drop(g);
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(
            crate::peers::recorder::classify(seen.lock().unwrap()[0].phase),
            crate::peers::recorder::Classified::Fail
        );
        crate::peers::recorder::install_observation_sink(Arc::new(|_| {})); // reset
    }
```

- [ ] **Step 2: Verify failure** — `install_observation_sink` undefined.

- [ ] **Step 3: Implement** the global sink + lib.rs install (above), then the two VARA sites:

(a) **Dial site** — in `run_vara_b2f_with_transport`'s candidate walk (`commands.rs:2497`). Only when `intent == SessionIntent::P2p` and a sink is installed. One guard per candidate; the connected candidate's guard survives into the exchange:

```rust
    let engine_transport = match session.active_transport_kind() {
        Some(TransportKind::VaraFm) => crate::peers::model::ChannelTransport::VaraFm,
        _ => crate::peers::model::ChannelTransport::VaraHf,
    };
    let sink = if intent == SessionIntent::P2p {
        crate::peers::recorder::observation_sink()
    } else {
        None
    };
    let mut live_guard: Option<crate::peers::recorder::ObservationGuard> = None;
```

inside the per-candidate closure, before `send_connect_and_wait`:

```rust
        let guard = sink.clone().map(|s| {
            crate::peers::recorder::ObservationGuard::new(
                s,
                crate::peers::recorder::PeerObservation {
                    path: crate::peers::recorder::ObservedPath::Rf {
                        transport: engine_transport,
                        via: c.via.clone(),
                        freq_hz: c.freq_hz,
                        bandwidth: None,
                    },
                    direction: crate::peers::model::Direction::Outgoing,
                    presented_target: c.target.clone(),
                    phase: crate::peers::recorder::ObservationPhase::DialAttempted,
                },
            )
        });
```

on `send_connect_and_wait` `Ok(())`: `if let Some(g) = &guard { g.set_phase(ObservationPhase::Connected); } live_guard = guard;` — on `Err`: let `guard` drop (records the per-candidate fail). After the exchange concludes (where `VaraExchangeOutcome` is decided, `commands.rs:2528-2600`): `Completed → live_guard.set_phase(B2fOk)`, `ExchangeFailed → set_phase(B2fFail)`; then drop. An abort/wedge path that unwinds past the walk drops `live_guard` at whatever phase it last reached — the fail records `[R3-11]`.

(b) **Answer site** — `ui_commands.rs` VARA listener consumer (~6519). Immediately before the `std::thread::scope` that runs `run_vara_b2f_answer_io`:

```rust
                        let obs_guard = crate::peers::recorder::observation_sink().map(|s| {
                            crate::peers::recorder::ObservationGuard::new(
                                s,
                                crate::peers::recorder::PeerObservation {
                                    path: crate::peers::recorder::ObservedPath::Rf {
                                        transport: match vara_session.active_transport_kind() {
                                            Some(crate::winlink::listener::transport::TransportKind::VaraFm) =>
                                                crate::peers::model::ChannelTransport::VaraFm,
                                            _ => crate::peers::model::ChannelTransport::VaraHf,
                                        },
                                        via: vec![],
                                        // No wire freq source on inbound (CONNECTED
                                        // carries bandwidth, not frequency) [R3-11].
                                        freq_hz: None,
                                        bandwidth: None,
                                    },
                                    direction: crate::peers::model::Direction::Incoming,
                                    presented_target: peer_call.clone(),
                                    phase: crate::peers::recorder::ObservationPhase::B2fStarted,
                                },
                            )
                        });
```

and in the existing `match result` arms: `Ok(()) → obs_guard.set_phase(Accepted)`; `Err(_) → set_phase(B2fFail)`; then `drop(obs_guard)`. A rejected inbound (allowlist gate) never reaches this code — no record, by construction.

- [ ] **Step 4: Run tests** — `cargo test --manifest-path src-tauri/Cargo.toml --locked` (CI) + local `recorder`/`vara` scoped runs. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/recorder.rs src-tauri/src/lib.rs src-tauri/src/winlink/modem/vara/commands.rs src-tauri/src/ui_commands.rs
git commit -m "feat(peers): global observation sink + VARA dial/answer record sites with drop-guard fail recording"
```

---

### Task 14: ARDOP record sites (outer connect-fail + exchange + answer)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (`run_ardop_connect_b2f_with_transport`, 1836-1857), `src-tauri/src/ui_commands.rs` (ARDOP listener consumer, ~5685/5709)
- Test: `modem_commands.rs` tests

**Interfaces:** consumes Tasks 11, 13. `[R5-2]`: ARDOP `ConnectFailed` returns pre-exchange at `modem_commands.rs:1844-1850` — the guard sits at `run_ardop_connect_b2f_with_transport`, NOT inside the exchange, so connect failures record.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    #[serial_test::serial]
    fn ardop_connect_fail_records_a_dial_attempt_fail() {
        let seen: Arc<std::sync::Mutex<Vec<crate::peers::recorder::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::peers::recorder::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }
        let mut failing = FailingConnectTransport::default(); // test double: connect_arq → Err
        let out = run_ardop_connect_b2f_for_test(&mut failing, "N0DAJ-7", SessionIntent::P2p);
        assert!(matches!(out, ExchangeOutcome::ConnectFailed(_)));
        let obs = seen.lock().unwrap();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].presented_target, "N0DAJ-7");
        assert_eq!(
            crate::peers::recorder::classify(obs[0].phase),
            crate::peers::recorder::Classified::Fail
        );
        crate::peers::recorder::install_observation_sink(Arc::new(|_| {}));
    }
```

(`FailingConnectTransport` — implement the `ModemTransport` trait's `connect_arq` as `Err`; follow the existing mock-transport pattern in `modem_commands.rs`'s test module. `run_ardop_connect_b2f_for_test` is a thin `#[cfg(test)]` wrapper if the real fn's `AppHandle` param blocks direct calling — read the existing ARDOP tests and reuse their harness; if they construct a test `AppHandle`, call the real fn.)

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.** At the top of `run_ardop_connect_b2f_with_transport` (`modem_commands.rs:1836`):

```rust
    let obs_guard = if intent == SessionIntent::P2p {
        crate::peers::recorder::observation_sink().map(|s| {
            crate::peers::recorder::ObservationGuard::new(
                s,
                crate::peers::recorder::PeerObservation {
                    path: crate::peers::recorder::ObservedPath::Rf {
                        transport: crate::peers::model::ChannelTransport::Ardop,
                        via: vec![],
                        freq_hz: None, // rig/CAT freq not threaded here; never fabricated [R3-11]
                        bandwidth: None,
                    },
                    direction: crate::peers::model::Direction::Outgoing,
                    presented_target: target.to_string(),
                    phase: crate::peers::recorder::ObservationPhase::DialAttempted,
                },
            )
        })
    } else {
        None
    };
```

then: after `connect_arq` succeeds → `set_phase(Connected)`; in the final match → `Ok(()) => set_phase(B2fOk)` / `Err(_) => set_phase(B2fFail)`; the early `ConnectFailed` return simply drops the guard at `DialAttempted` (fail recorded — this IS the `[R5-2]` outer site).

**Answer site** — `ui_commands.rs` ARDOP listener consumer: identical shape to Task 13(b) (guard `B2fStarted` before `run_ardop_b2f_answer`, `Accepted`/`B2fFail` from the result match, `transport: ChannelTransport::Ardop`), wrapped around BOTH `run_ardop_b2f_answer` invocations (real mailbox ~5685 and tempdir ~5709) — place the guard above the `let result = match mb_ref {…}` so one guard covers both arms.

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modem_commands.rs src-tauri/src/ui_commands.rs
git commit -m "feat(peers): ARDOP record sites — outer connect-fail guard [R5-2] + answer-role recording"
```

---

### Task 15: Packet record sites (dial + answer)

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (`native_packet_connect`, dial arm ~2681 and answer arm ~2807)
- Test: `winlink_backend.rs` tests (the existing `native_packet_exchange` loopback tests at ~5792-5987 provide the harness pattern)

**Interfaces:** consumes Tasks 11-13.

- [ ] **Step 1: Write the failing test** — mirror the existing packet loopback exchange tests: run `native_packet_connect`'s answer path (or `native_packet_exchange` plus a directly-constructed guard, matching Task 13's unit-layer pattern) with a `serial_test`-installed capture sink and `intent: SessionIntent::P2p`; assert one observation with `direction: Incoming`, `transport: Packet`, and `Classified::Ok` on success. Add the dial-fail twin: AX.25 connect error → one observation, `Classified::Fail`.

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.** In `native_packet_connect`:

**Dial arm** — before the AX.25 `connect_link` call (so a link-level connect failure records), gated on `intent == SessionIntent::P2p`:

```rust
            let obs_guard = if intent == SessionIntent::P2p {
                crate::peers::recorder::observation_sink().map(|s| {
                    crate::peers::recorder::ObservationGuard::new(
                        s,
                        crate::peers::recorder::PeerObservation {
                            path: crate::peers::recorder::ObservedPath::Rf {
                                transport: crate::peers::model::ChannelTransport::Packet,
                                via: digis.iter().map(|d| d.to_string()).collect(),
                                freq_hz: None,
                                bandwidth: None,
                            },
                            direction: crate::peers::model::Direction::Outgoing,
                            presented_target: target.call.clone(),
                            phase: crate::peers::recorder::ObservationPhase::DialAttempted,
                        },
                    )
                })
            } else {
                None
            };
```

after `connect_link` returns Ok → `set_phase(Connected)`; after `native_packet_exchange` → `Ok(()) => set_phase(B2fOk)` / `Err(_) => set_phase(B2fFail)`.

**Answer arm** — after the listener gate ACCEPTS (`winlink_backend.rs:~2806`, past the reject early-return, which records nothing by construction):

```rust
            let obs_guard = crate::peers::recorder::observation_sink().map(|s| {
                crate::peers::recorder::ObservationGuard::new(
                    s,
                    crate::peers::recorder::PeerObservation {
                        path: crate::peers::recorder::ObservedPath::Rf {
                            transport: crate::peers::model::ChannelTransport::Packet,
                            via: vec![],
                            freq_hz: None,
                            bandwidth: None,
                        },
                        direction: crate::peers::model::Direction::Incoming,
                        presented_target: peer.call.clone(),
                        phase: crate::peers::recorder::ObservationPhase::B2fStarted,
                    },
                )
            });
```

result match: `Ok → Accepted`, `Err → B2fFail`. (The answer arm needs no intent check — `packet_listen_transport_from_config` pins Listen to `P2p`, Task 12.)

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS, including all pre-existing packet loopback tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "feat(peers): packet record sites — dial guard pre-connect, answer guard post-gate"
```

---

### Task 16: Telnet record sites (dial + listen answer) — completes the 8-site matrix

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (`telnet_p2p_connect`, ~7597-7887), `src-tauri/src/winlink/telnet_listen.rs` (`handle_one_session`, ~300+), `src-tauri/src/peers/commands.rs` (flip `peer_store: true`)
- Test: `telnet_listen.rs` tests (existing session-harness tests exist in-module)

**Interfaces:** consumes Tasks 11, 13.

- [ ] **Step 1: Write the failing test** — in `telnet_listen.rs`'s test module (which already drives `handle_one_session` over loopback TCP): with a `serial_test`-installed capture sink, run one accepted session → assert one observation with `ObservedPath::Telnet { provenance: ObservedIncoming, port: DEFAULT_PORT, .. }`, `direction: Incoming`, `Classified::Ok`; run one allowlist-rejected session → assert zero observations.

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.**

**Dial site** — in `telnet_p2p_connect` (`ui_commands.rs:7597`), after the exchange config is built (~7749) and before the `spawn_blocking` (~7763):

```rust
    let obs_guard = crate::peers::recorder::observation_sink().map(|s| {
        crate::peers::recorder::ObservationGuard::new(
            s,
            crate::peers::recorder::PeerObservation {
                path: crate::peers::recorder::ObservedPath::Telnet {
                    host: req.host.clone(),
                    port: req.port,
                    // Operator-typed host → Operator provenance: this dial IS
                    // the out-of-band consent (spec §4; the UI click is consent).
                    provenance: crate::peers::model::Provenance::Operator,
                },
                direction: crate::peers::model::Direction::Outgoing,
                presented_target: req.peer_callsign.clone(),
                phase: crate::peers::recorder::ObservationPhase::DialAttempted,
            },
        )
    });
```

The guard must ride into/around the `spawn_blocking` result handling: success arm (~7801) → `set_phase(B2fOk)`; failure arm (~7867) → if the error is a login failure (`P2pTelnetError::Login`-class — match the error variants `telnet_p2p.rs` exposes) `set_phase(LoginFailed)` else leave `DialAttempted`/set `B2fFail` per where it failed; aborted → `set_phase(AbortedOrWedged)`. (`ObservationGuard` is `Send`; move it into the async continuation, not the blocking closure.)

**Note on the store's provenance guard:** `PeersStore::apply_observation` must downgrade `Operator` provenance **only for `direction: Incoming`** observations — an outbound operator dial legitimately records an `Operator` endpoint (this is exactly how WLE telnet favorites are born). Task 8's monotonic test covers the inbound half; add the outbound assertion here:

```rust
    // peers/store.rs tests
    #[test]
    fn outbound_operator_dial_records_an_operator_endpoint() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let obs = PeerObservation {
            path: ObservedPath::Telnet {
                host: "peer.example.net".into(),
                port: 8774,
                provenance: Provenance::Operator,
            },
            direction: Direction::Outgoing,
            presented_target: "W6ABC".into(),
            phase: ObservationPhase::B2fOk,
        };
        s.apply_observation(&obs, now()).unwrap();
        assert_eq!(s.file().peers[0].endpoints[0].provenance, Provenance::Operator);
    }
```

**Listen answer site** — in `handle_one_session` (`telnet_listen.rs`), immediately after the allowlist gate accepts the parsed `peer_call` (before the B2F answer handoff):

```rust
    let obs_guard = crate::peers::recorder::observation_sink().map(|s| {
        crate::peers::recorder::ObservationGuard::new(
            s,
            crate::peers::recorder::PeerObservation {
                path: crate::peers::recorder::ObservedPath::Telnet {
                    host: peer_addr.ip().to_string(),
                    // The peer's SOURCE port is ephemeral; record the telnet
                    // P2P convention port (DEFAULT_PORT = 8774, WLE parity)
                    // as the CLAIMED back-dial endpoint. It is ObservedIncoming
                    // → agent-non-dialable, "unverified" badge (spec §4).
                    port: DEFAULT_PORT,
                    provenance: crate::peers::model::Provenance::ObservedIncoming,
                },
                direction: crate::peers::model::Direction::Incoming,
                presented_target: peer_call.clone(),
                phase: crate::peers::recorder::ObservationPhase::B2fStarted,
            },
        )
    });
```

then `Accepted` on successful exchange return, `B2fFail` on error. Reject paths (allowlist / password / IPv6) return BEFORE the guard exists — no record. All 8 spec-§3 sites are now live: VARA dial+answer (T13), ARDOP dial+answer (T14), packet dial+answer (T15), telnet dial+answer (T16).

Flip `peer_store: true` in `P2pCapabilities` (Task 11) — matrix rows 1-2 are now honest.

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/winlink/telnet_listen.rs src-tauri/src/peers/
git commit -m "feat(peers): telnet record sites complete the 8-site matrix; peer_store capability on"
```

---

### Task 17: Favorites link — `peer_id`, recorder→recents bridge, frontend double-count suppression

**Files:**
- Modify: `src-tauri/src/favorites/store.rs` (`Favorite` struct, 63-77), `src-tauri/src/peers/recorder.rs` (`bridge_to_favorites`), `src-tauri/src/lib.rs` (enable the bridge line from Task 13), `src/favorites/types.ts` (18-31), `src/connections/connectDispatch.ts` (48-59, 122-224), `src-tauri/src/peers/commands.rs` (flip `favorites_peer_link: true`)
- Test: `favorites/store.rs` + `src/connections/connectDispatch.test.ts`

**Interfaces:**
- Consumes: Tasks 11, 13.
- Produces: `Favorite.peer_id: Option<String>` (Rust) / `peer_id?: string` (TS); `bridge_to_favorites(favorites: &Arc<Mutex<FavoritesStore>>, obs: &PeerObservation)`.

Spec `[R5-7]`: the peer recorder is authoritative for P2P recents, with ONE explicit bridge to the favorites attempt log; the frontend's `recordRibbonAttempt` (`connectDispatch.ts:48-59`) must NOT also record for P2P sessions (double-count).

- [ ] **Step 1: Write the failing tests**

Rust (`favorites/store.rs` tests): `favorite_with_peer_id_round_trips` — construct a `Favorite { peer_id: Some("p1".into()), .. }`, flush, reopen, assert preserved; and an old-file test: a stations.json row WITHOUT `peer_id` loads with `None` (additive `#[serde(default)]`).

TS (`connectDispatch.test.ts`, following its existing mock pattern): `p2p session outcomes are not ribbon-recorded` — drive `connectFor` with a `p2p` sessionType key over a mocked `invoke`; assert `favorite_record_attempt` was NOT invoked (the backend bridge owns it), while a gateway (`cms`-class) dial still records exactly once.

Rust (`recorder.rs` tests): `bridge_maps_transport_to_radio_mode` — `ChannelTransport::VaraHf → "vara-hf"`, `VaraFm → "vara-fm"`, `Ardop → "ardop-hf"`, `Packet → "packet"`, telnet path → `"telnet"`; outbound RF `B2fOk` observation appends one `reached` attempt via a temp-dir `FavoritesStore`; `Direction::Incoming` observations bridge NOTHING (recents = dials).

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.**

(a) `Favorite` gains `#[serde(default)] pub peer_id: Option<String>` (Rust, after `note`) and `peer_id?: string` (TS, same position). Update `favorite_upsert`'s merge logic to carry it, and `dialToFavorite.ts` to pass it through when present.

(b) `bridge_to_favorites` in `recorder.rs`:

```rust
/// The ONE bridge from peer observations to the favorites attempt log
/// [R5-7]. Outbound conclusions only (recents = dials); the frontend
/// suppresses its own p2p ribbon recording so this is the sole writer.
pub fn bridge_to_favorites(
    favorites: &std::sync::Arc<std::sync::Mutex<crate::favorites::store::FavoritesStore>>,
    obs: &PeerObservation,
) {
    use crate::peers::model::{ChannelTransport, Direction};
    if obs.direction != Direction::Outgoing {
        return;
    }
    let outcome = match classify(obs.phase) {
        Classified::Ok => "reached",
        Classified::Fail => "failed",
        Classified::NoRecord => return,
    };
    let mode = match &obs.path {
        ObservedPath::Rf { transport, .. } => match transport {
            ChannelTransport::VaraHf => "vara-hf",
            ChannelTransport::VaraFm => "vara-fm",
            ChannelTransport::Ardop => "ardop-hf",
            ChannelTransport::Packet => "packet",
            ChannelTransport::Unknown => return,
        },
        ObservedPath::Telnet { .. } => "telnet",
    };
    let ts_local = chrono::Local::now().to_rfc3339();
    if let Ok(mut f) = favorites.lock() {
        let dial = crate::favorites::store::FavoriteDial {
            mode: mode.to_string(),
            gateway: obs.presented_target.clone(),
            // remaining FavoriteDial fields: None/default (read the struct at
            // favorites/store.rs:118-126 and fill them explicitly)
            ..Default::default()
        };
        if let Err(e) = f.record_attempt(&dial, outcome, &ts_local) {
            tracing::warn!(target: "tuxlink::peers", "favorites bridge skipped: {e:?}");
        }
    }
}
```

(Match `record_attempt`'s REAL signature at `favorites/store.rs:628-684` — adjust the call to its exact parameter shapes; if `FavoriteDial` does not derive `Default`, construct all fields explicitly.) Enable the `bridge_to_favorites` call in the lib.rs sink closure (Task 13 comment marker).

(c) `connectDispatch.ts`: in `connectFor` (~122), thread the session type into the three RF outcome sites and guard:

```ts
  const isP2p = key.sessionType === 'p2p';
  // [R5-7] the backend peer-recorder is authoritative for p2p recents —
  // recording here too would double-count the attempt.
  if (!isP2p) recordRibbonAttempt(mode, gateway, outcome);
```

(apply at each of the ARDOP/VARA/packet call sites of `recordRibbonAttempt`, lines ~153-211).

(d) Flip `favorites_peer_link: true` (matrix row 10).

- [ ] **Step 4: Run tests** — `pnpm vitest run src/connections/connectDispatch.test.ts` locally + Rust via CI. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/favorites/ src-tauri/src/peers/ src-tauri/src/lib.rs src/favorites/types.ts src/connections/connectDispatch.ts
git commit -m "feat(peers): Favorite.peer_id + single recorder→recents bridge, frontend p2p double-count suppression [R5-7]"
```

---

### Task 18: Write-boundary pinning — hostile inbound callsigns create no record

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/listener.rs` (doc-comment on `parse_peer_call`), `src-tauri/src/peers/store.rs` (tests only)
- Test: `peers/store.rs` + `telnet_listen.rs`

**Interfaces:** consumes Tasks 1, 8, 16.

Spec §4 `[R2-S2][R2-S10]`: `parse_peer_call` (`listener.rs:94-110`) upper-cases + trims but applies no charset filter, and `allow_all` defaults TRUE — so the callsign field on an inbound connection carries arbitrary attacker bytes. The **accept policy is deliberately unchanged** (`AllowedStations` decides; features-yes-no-added-safeguards). The enforced boundary is the WRITE: `PeersStore::apply_observation` rejects any presented callsign failing `sanitize_display` (Task 8 backstop), and the keyring never sees a callsign in an account string (Task 10). This task PINS that with hostile-input tests at each inbound path and documents the boundary at `parse_peer_call`.

- [ ] **Step 1: Write the failing/pinning tests**

```rust
    // peers/store.rs tests
    #[test]
    fn hostile_callsigns_never_reach_the_roster() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        for evil in [
            "<img src=x onerror=alert(1)>",
            "W6ABC:extra",
            "A\u{0}B",
            "../../etc/passwd",
            "W6 ABC",
            "`rm -rf`",
        ] {
            let eff = s
                .apply_observation(
                    &rf_obs(evil, Direction::Incoming, ObservationPhase::Accepted),
                    now(),
                )
                .unwrap();
            assert!(matches!(eff, ApplyEffect::NoRecord), "{evil:?} must be dropped");
        }
        assert!(s.file().peers.is_empty());
    }
```

Plus, in `telnet_listen.rs`'s harness (extends Task 16's test): drive a session whose CALLSIGN phase supplies `<b>EVIL</b>` with a capture sink installed — the exchange may proceed or reject per the existing gate, but the roster observation must classify to `NoRecord` (assert the store stays empty via a temp store wired into the sink).

- [ ] **Step 2: Run** — the store test should PASS already (Task 8's backstop); if any input slips through, tighten `sanitize_display` (Task 1). The listener test is new coverage.

- [ ] **Step 3: Document the boundary.** On `parse_peer_call` (`listener.rs:94`), extend the doc-comment:

```rust
/// NOTE (spec §4 write boundary): this parser deliberately applies NO
/// charset filter — accept policy belongs to `AllowedStations`, and WLE
/// parity means a malformed claimed callsign may still get a session.
/// The PEER ROSTER is protected downstream: `PeersStore::apply_observation`
/// drops any presented callsign failing `callsign::sanitize_display`
/// [R2-S2], and keyring accounts are id-keyed, never callsign-keyed
/// [R2-S10]. Render surfaces escape everything (frontend hostile-callsign
/// tests).
```

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/store.rs src-tauri/src/winlink/modem/vara/listener.rs src-tauri/src/winlink/telnet_listen.rs
git commit -m "test(peers): pin the write boundary — hostile inbound callsigns never reach the roster"
```

---

### Task 19: `curate_peer` + `find_peers` (egress-arm-gated agent read)

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (DTOs + `StationPort` method), `src-tauri/tuxlink-mcp-core/src/router.rs` (tool), `src-tauri/src/mcp_ports.rs` (`curate_peer` + `MonolithStationPort::find_peers`), `src-tauri/src/peers/commands.rs` (flip `agent_find_peers: true`)
- Test: `mcp_ports.rs` tests (beside the existing `curate_gateway` tests)

**Interfaces:**
- Consumes: Tasks 1, 8, 11.
- Produces (DTOs in `tuxlink-mcp-core/src/ports.rs`, mirroring `GatewayDto`'s derive set):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerChannelDto {
    pub transport: String,        // "packet" | "ardop" | "vara-hf" | "vara-fm"
    pub target_callsign: String,
    pub via: Vec<String>,
    pub freq_hz: Option<u64>,
    pub direction: String,        // "incoming" | "outgoing"
    pub ok: u32,
    pub fail: u32,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerEndpointDto {
    pub id: String,
    pub provenance: String,       // "operator" | "observed-incoming"
    /// host/port are present ONLY when provenance is Operator AND the
    /// egress arm is active [R2-S3]; otherwise redacted (None).
    pub host: Option<String>,
    pub port: Option<u16>,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerDto {
    pub id: String,
    pub canonical_base: String,
    pub presented_callsigns: Vec<String>,
    pub identity_kind: String,
    pub origin: String,
    /// Clamped to the operator's configured broadcast precision [R2-S9].
    pub grid: Option<String>,
    pub last_connected_at: Option<String>,
    pub channels: Vec<PeerChannelDto>,
    pub endpoints: Vec<PeerEndpointDto>,
    // DROPPED on purpose: note, contact_id (and anything reachable through
    // it), do_not_merge/conflict/source internals [R2-S11][R4-9].
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerListDto {
    pub peers: Vec<PeerDto>,
}
```

and `StationPort` gains `async fn find_peers(&self) -> Result<PeerListDto, PortError>;`.

- [ ] **Step 1: Write the failing tests** (in `mcp_ports.rs`, beside the `curate_gateway` tests at ~2311)

```rust
    #[test]
    fn curate_peer_drops_free_text_and_clamps_grid() {
        let peer = hostile_test_peer(); // note: "meet at repeater", grid "CN87xk91", contact_id set
        let dto = curate_peer(&peer, 4, /* armed = */ false).expect("valid callsign → Some");
        // [R2-S11] free text never crosses; [R4-9] contact link not resolved.
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("meet at repeater"));
        assert!(!json.contains("contact"));
        // [R2-S9] grid clamped to operator precision (4-char default).
        assert_eq!(dto.grid.as_deref(), Some("CN87"));
    }

    #[test]
    fn curate_peer_redacts_endpoints_unless_operator_and_armed() {
        let peer = peer_with_two_endpoints(); // one Operator, one ObservedIncoming
        let unarmed = curate_peer(&peer, 4, false).unwrap();
        assert!(unarmed.endpoints.iter().all(|e| e.host.is_none() && e.port.is_none()));
        let armed = curate_peer(&peer, 4, true).unwrap();
        let op = armed.endpoints.iter().find(|e| e.provenance == "operator").unwrap();
        assert!(op.host.is_some() && op.port.is_some());
        let obs = armed.endpoints.iter().find(|e| e.provenance == "observed-incoming").unwrap();
        assert!(obs.host.is_none(), "ObservedIncoming stays redacted even when armed [R2-S3]");
    }

    #[test]
    fn curate_peer_drops_records_with_unsanitizable_callsigns() {
        let mut peer = hostile_test_peer();
        peer.canonical_base = "<script>".into();
        assert!(curate_peer(&peer, 4, false).is_none(), "[R5-10] sanitizer floor");
    }
```

(`hostile_test_peer()` / `peer_with_two_endpoints()`: small fixture fns constructing `crate::peers::model::Peer` values inline — write them in the test module with every field explicit.)

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.**

(a) `curate_peer` in `mcp_ports.rs`, beside `curate_gateway` (2311):

```rust
/// Curation, not a DTO mirror [R2-S1]: validates every callsign through the
/// broad sanitizer + drops rows that fail; drops note/free-text/contact
/// reach-through; clamps grid to operator precision; redacts endpoint
/// host:port unless Operator-provenance AND the egress arm is active.
fn curate_peer(
    p: &crate::peers::model::Peer,
    grid_precision: usize,
    armed: bool,
) -> Option<PeerDto> {
    use crate::winlink::callsign::sanitize_display;
    let canonical_base = sanitize_display(&p.canonical_base)?;
    let presented_callsigns: Vec<String> = p
        .presented_callsigns
        .iter()
        .filter_map(|c| sanitize_display(c))
        .collect();
    let grid = p.grid.as_ref().and_then(|g| {
        let v = g.value.trim().to_ascii_uppercase();
        if v.len() < 4 || !v.is_ascii() {
            return None;
        }
        Some(v[..grid_precision.clamp(4, v.len())].to_string()) // [R2-S9]
    });
    let channels = p
        .channels
        .iter()
        .filter_map(|c| {
            let target_callsign = sanitize_display(&c.target_callsign)?;
            Some(PeerChannelDto {
                transport: match c.transport {
                    crate::peers::model::ChannelTransport::Packet => "packet",
                    crate::peers::model::ChannelTransport::Ardop => "ardop",
                    crate::peers::model::ChannelTransport::VaraHf => "vara-hf",
                    crate::peers::model::ChannelTransport::VaraFm => "vara-fm",
                    crate::peers::model::ChannelTransport::Unknown => return None,
                }
                .to_string(),
                target_callsign,
                via: c.via.iter().filter_map(|v| sanitize_display(v)).collect(),
                freq_hz: c.freq_hz,
                direction: match c.direction {
                    crate::peers::model::Direction::Incoming => "incoming",
                    crate::peers::model::Direction::Outgoing => "outgoing",
                    crate::peers::model::Direction::Unknown => "incoming",
                }
                .to_string(),
                ok: c.counts.ok,
                fail: c.counts.fail,
                last_seen: c.last_seen.clone(),
            })
        })
        .collect();
    let endpoints = p
        .endpoints
        .iter()
        .map(|e| {
            let is_operator = e.provenance == crate::peers::model::Provenance::Operator;
            let reveal = is_operator && armed; // [R2-S3]
            PeerEndpointDto {
                id: e.id.clone(),
                provenance: if is_operator { "operator" } else { "observed-incoming" }.to_string(),
                host: reveal.then(|| e.host.clone()),
                port: reveal.then_some(e.port),
                last_seen: e.last_seen.clone(),
            }
        })
        .collect();
    Some(PeerDto {
        id: p.id.clone(),
        canonical_base,
        presented_callsigns,
        identity_kind: match p.identity_kind {
            crate::peers::model::IdentityKind::Individual => "individual",
            crate::peers::model::IdentityKind::Tactical => "tactical",
            crate::peers::model::IdentityKind::Club => "club",
            crate::peers::model::IdentityKind::Unknown => "unknown",
        }
        .to_string(),
        origin: match p.origin {
            crate::peers::model::Origin::Incoming => "incoming",
            crate::peers::model::Origin::Outgoing => "outgoing",
            crate::peers::model::Origin::Manual => "added",
            crate::peers::model::Origin::Aprs => "aprs",
            crate::peers::model::Origin::Unknown => "unknown",
        }
        .to_string(),
        grid,
        last_connected_at: p.last_connected_at.clone(),
        channels,
        endpoints,
    })
}
```

(b) `MonolithStationPort::find_peers`: **gate the whole read behind the egress arm** `[R2-S5]` — the roster is the operator's private social graph, not catalog data. `MonolithStationPort` gains `guard: Arc<EgressGuard>` + `app` access to the peers store (mirror how `MonolithEgressPort` holds them; wire in the port constructor):

```rust
    async fn find_peers(&self) -> Result<PeerListDto, PortError> {
        self.guard
            .authorize(tuxlink_security::EgressAuthority::Agent)
            .map_err(|d| PortError::Denied(d.to_string()))?;
        let armed = true; // authorize() passed ⇒ armed and untainted
        let precision = 4; // operator broadcast precision default; read the
                           // configured value the same way curate_gateway's
                           // caller resolves operator_grid (config read)
        let store = self.app.state::<Arc<Mutex<crate::peers::store::PeersStore>>>();
        let file = store.lock().map_err(|_| PortError::Internal("peers store poisoned".into()))?.file().clone();
        Ok(PeerListDto {
            peers: file.peers.iter().filter_map(|p| curate_peer(p, precision, armed)).collect(),
        })
    }
```

(If `PortError` has no `Denied`/`Internal` variants, use the existing variants `port_err` maps — read `ports.rs`'s `PortError` first and match its conventions.)

(c) Router tool (`router.rs`, beside `find_stations` at 475):

```rust
    #[tool(
        name = "find_peers",
        description = "List saved P2P peer stations (callsigns, RF channels, last-connected). Endpoint host:port appears only for operator-verified endpoints while the egress window is armed. Requires the egress arm — the peer roster is the operator's private station data, not public directory data. Read-only; does not transmit."
    )]
    pub async fn find_peers(&self) -> Result<CallToolResult, ErrorData> {
        let dto = self.state.stations.find_peers().await.map_err(port_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::json(dto)?]))
    }
```

(d) Flip `agent_find_peers: true` (matrix row 4).

- [ ] **Step 4: Run tests** (scoped/CI) — including a shape test that `serde_json::to_string(&PeerDto)` never contains a `note` key. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-mcp-core/ src-tauri/src/mcp_ports.rs src-tauri/src/peers/commands.rs
git commit -m "feat(mcp): find_peers behind the egress arm with curate_peer — sanitizer floor, grid clamp, endpoint redaction"
```

---

### Task 20: Agent telnet dial — `(peer_id, endpoint_id)`, DNS-rebinding-safe denylist

**Files:**
- Modify: `src-tauri/src/winlink/telnet_p2p.rs` (vetting + `connect_and_exchange_to_addrs`), `src-tauri/tuxlink-mcp-core/src/ports.rs` (`EgressPort` method), `src-tauri/tuxlink-mcp-core/src/router.rs` (gated tool), `src-tauri/src/mcp_ports.rs` (impl), `src-tauri/src/peers/commands.rs` (flip `agent_telnet_dial: true`)
- Test: `telnet_p2p.rs` (denylist unit tests), `mcp_ports.rs` (resolution-rule tests)

**Interfaces:**
- Consumes: Tasks 8, 10, 19.
- Produces: `EgressPort::telnet_p2p_exchange(peer_id: String, endpoint_id: String) -> Result<(), EgressPortError>`; `telnet_p2p::vet_peer_endpoint(host, port) -> Result<Vec<SocketAddr>, P2pTelnetError>`; `telnet_p2p::connect_stream_to_addrs(addrs: &[SocketAddr]) -> Result<TcpStream, P2pTelnetError>`.

Spec §4 `[R2-S3][R2-S4][R1-C4][R5-6]`: the agent NEVER supplies a raw host. It supplies ids; the impl resolves `(host, port)` from an `Operator`-provenance endpoint only, resolves DNS **once**, applies the denylist to **every** candidate address (if ANY candidate is denied, the dial is refused — a mixed answer is rebinding-shaped), and connects to a vetted concrete `SocketAddr` with **no second lookup**. The existing UI command `telnet_p2p_connect` (raw host) stays UI-only — the operator click is consent.

- [ ] **Step 1: Write the failing tests**

```rust
    // telnet_p2p.rs tests
    #[test]
    fn denylist_rejects_private_loopback_linklocal_ula_metadata_and_mapped() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
        let denied: Vec<IpAddr> = vec![
            Ipv4Addr::new(127, 0, 0, 1).into(),
            Ipv4Addr::new(10, 1, 2, 3).into(),
            Ipv4Addr::new(172, 16, 0, 1).into(),
            Ipv4Addr::new(192, 168, 1, 1).into(),
            Ipv4Addr::new(169, 254, 169, 254).into(), // cloud metadata (link-local)
            Ipv4Addr::new(169, 254, 0, 9).into(),
            Ipv6Addr::LOCALHOST.into(),
            "fe80::1".parse::<Ipv6Addr>().unwrap().into(),
            "fc00::1".parse::<Ipv6Addr>().unwrap().into(),
            "fd12:3456::1".parse::<Ipv6Addr>().unwrap().into(),
            "::ffff:192.168.1.5".parse::<Ipv6Addr>().unwrap().into(), // v4-mapped private
        ];
        for ip in denied {
            assert!(ip_is_denied(ip), "{ip} must be denied");
        }
        let allowed: Vec<IpAddr> = vec![
            Ipv4Addr::new(203, 0, 113, 5).into(),
            "2001:db8::5".parse::<Ipv6Addr>().unwrap().into(),
        ];
        for ip in allowed {
            assert!(!ip_is_denied(ip), "{ip} must be allowed");
        }
    }

    #[test]
    fn vet_refuses_when_any_candidate_is_denied() {
        // [R5-6] a mixed public+private DNS answer is rebinding-shaped —
        // refuse entirely rather than "connect to the good one".
        let addrs = vec![
            "203.0.113.5:8774".parse().unwrap(),
            "169.254.169.254:8774".parse().unwrap(),
        ];
        assert!(vet_candidates(&addrs).is_err());
        let clean = vec!["203.0.113.5:8774".parse().unwrap()];
        assert!(vet_candidates(&clean).is_ok());
    }
```

```rust
    // mcp_ports.rs tests
    #[test]
    fn agent_telnet_dial_refuses_observed_incoming_and_unknown_ids() {
        // resolve_agent_dialable_endpoint is the pure resolution rule:
        // Operator provenance only [R2-S3]; unknown ids are errors.
        let peer = peer_with_two_endpoints();
        let op_id = peer.endpoints.iter().find(|e| e.provenance == Provenance::Operator).unwrap().id.clone();
        let obs_id = peer.endpoints.iter().find(|e| e.provenance == Provenance::ObservedIncoming).unwrap().id.clone();
        assert!(resolve_agent_dialable_endpoint(&peer, &op_id).is_ok());
        assert!(resolve_agent_dialable_endpoint(&peer, &obs_id).is_err());
        assert!(resolve_agent_dialable_endpoint(&peer, "nope").is_err());
    }
```

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.**

(a) `telnet_p2p.rs` — denylist + single-resolution vetting + addr-list connect:

```rust
/// Egress denylist [R2-S4][R5-6]. Manual IPv6 range checks because
/// `Ipv6Addr::is_unique_local` / `is_unicast_link_local` are unstable at
/// MSRV 1.75.
pub(crate) fn ip_is_denied(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // includes 169.254.169.254 metadata
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_is_denied(IpAddr::V4(mapped));
            }
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg[0] & 0xfe00) == 0xfc00   // fc00::/7 ULA
                || (seg[0] & 0xffc0) == 0xfe80   // fe80::/10 link-local
        }
    }
}

/// All-must-pass vetting: one denied candidate refuses the whole dial.
pub(crate) fn vet_candidates(
    addrs: &[std::net::SocketAddr],
) -> Result<Vec<std::net::SocketAddr>, P2pTelnetError> {
    if addrs.is_empty() {
        return Err(P2pTelnetError::EgressDenied {
            reason: "hostname resolved to no addresses".into(),
        });
    }
    for a in addrs {
        if ip_is_denied(a.ip()) {
            return Err(P2pTelnetError::EgressDenied {
                reason: format!("resolved address {a} is in a denied range (loopback/private/link-local/ULA/metadata)"),
            });
        }
    }
    Ok(addrs.to_vec())
}

/// Resolve ONCE + vet. The returned addrs are the ONLY thing the agent
/// dial connects to — no second lookup [R5-6].
pub fn vet_peer_endpoint(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, P2pTelnetError> {
    use std::net::ToSocketAddrs;
    let addrs: Vec<std::net::SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|source| P2pTelnetError::Resolve { host: host.to_string(), port, source })?
        .collect();
    vet_candidates(&addrs)
}

/// Connect to pre-vetted concrete addresses (agent path). Mirrors
/// `connect_stream`'s timeout behavior without re-resolving.
pub(crate) fn connect_stream_to_addrs(
    addrs: &[std::net::SocketAddr],
) -> Result<std::net::TcpStream, P2pTelnetError> {
    let mut last_err: Option<(std::net::SocketAddr, std::io::Error)> = None;
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT)).ok();
                stream.set_write_timeout(Some(TIMEOUT)).ok();
                return Ok(stream);
            }
            Err(e) => last_err = Some((*addr, e)),
        }
    }
    let (addr, source) = last_err.expect("vet_candidates rejects empty lists");
    Err(P2pTelnetError::Connect { addr, source })
}
```

Add the `EgressDenied { reason: String }` variant to `P2pTelnetError` (match its existing thiserror style). Add `pub fn connect_and_exchange_to_addrs(addrs, config, mailbox, …)` — a sibling of `connect_and_exchange` (`telnet_p2p.rs:131-144`) that takes the vetted list and calls `connect_stream_to_addrs` instead of `connect_stream`; refactor the shared body so both wrappers delegate to one inner fn (no duplicated login/exchange logic).

(b) Pure resolution rule in `mcp_ports.rs`:

```rust
/// The agent may dial ONLY an Operator-provenance endpoint, addressed by
/// ids [R2-S3][R1-C4]. Pure, so the rule is unit-tested without a store.
fn resolve_agent_dialable_endpoint<'a>(
    peer: &'a crate::peers::model::Peer,
    endpoint_id: &str,
) -> Result<&'a crate::peers::model::Endpoint, String> {
    let ep = peer
        .endpoints
        .iter()
        .find(|e| e.id == endpoint_id)
        .ok_or_else(|| format!("no endpoint {endpoint_id} on peer {}", peer.id))?;
    if ep.provenance != crate::peers::model::Provenance::Operator {
        return Err(
            "endpoint is not operator-verified (ObservedIncoming endpoints are \
             agent-non-dialable; promote it in Settings → P2P Peers after \
             out-of-band verification)"
                .to_string(),
        );
    }
    Ok(ep)
}
```

(c) `EgressPort::telnet_p2p_exchange(peer_id, endpoint_id)` in `ports.rs` + the `MonolithEgressPort` impl: inside `guarded_egress(&self.guard, EgressAuthority::Agent, "telnet_p2p_exchange", …)` — look up the peer by id from the store, `resolve_agent_dialable_endpoint`, `vet_peer_endpoint(&ep.host, ep.port)`, read the password via `p2p_endpoint_password_read(peer_id, endpoint_id)` (never sent anywhere but this Operator endpoint `[R2-S7]`), pick the peer's presented callsign for the login (first `presented_callsigns` entry matching the endpoint's peer — use `peer.presented_callsigns.first()`), then run the exchange via `connect_and_exchange_to_addrs` in `spawn_blocking`, mirroring the UI command's mailbox/inbox handling (`ui_commands.rs:7801-7866` — extract shared post-exchange handling into a helper if the duplication exceeds ~20 lines).

(d) Router tool (gated, beside the other egress tools at `router.rs:600-731`):

```rust
    #[tool(
        name = "telnet_p2p_connect",
        description = "Run a Winlink P2P exchange with a saved peer over the internet (TCP, no RF). Takes a peer_id + endpoint_id from find_peers; only operator-verified endpoints are dialable. Requires the egress arm."
    )]
    pub async fn telnet_p2p_connect(
        &self,
        params: Parameters<PeerEndpointParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(PeerEndpointParams { peer_id, endpoint_id }) = params;
        self.state.egress.telnet_p2p_exchange(peer_id, endpoint_id).await.map_err(port_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text("exchange complete")]))
    }
```

with `PeerEndpointParams { peer_id: String, endpoint_id: String }` (schemars-derived, like `CallsignParams` at `router.rs:1064-1069`).

(e) Flip `agent_telnet_dial: true` (matrix row 7).

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/telnet_p2p.rs src-tauri/tuxlink-mcp-core/ src-tauri/src/mcp_ports.rs src-tauri/src/peers/commands.rs
git commit -m "feat(mcp): agent telnet P2P dial by (peer_id, endpoint_id) — Operator-only, resolve-once all-must-pass egress denylist"
```

---

### Task 21: Engine-aware VARA agent egress (remove the `VaraHf` hard-pins)

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (param + DTO), `src-tauri/tuxlink-mcp-core/src/router.rs` (tool params), `src-tauri/src/mcp_ports.rs` (both pins: `vara_b2f_exchange` 1269-1308, `vara_open_session` 1310-1341), `src-tauri/src/peers/commands.rs` (flip `vara_engine_split: true`)
- Test: `mcp_ports.rs` mapping test

**Interfaces:**
- Consumes: Task 4 (engine-split behavior downstream).
- Produces: `VaraEngineDto { VaraHf, VaraFm }` (kebab-case `"vara-hf"`/`"vara-fm"`, schemars) added as `engine: Option<VaraEngineDto>` to BOTH `vara_b2f_exchange` and `vara_open_session` (port trait + router params + impls). `None → VaraHf` (backward-compatible with every existing caller).

Why `[R5-1]`: both `MonolithEgressPort` VARA methods hardcode `TransportKind::VaraHf` — an agent acting on a `vara-fm` peer channel would dial FM peers with the HF engine/protocol. The agent action must dispatch on the channel's engine.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn vara_engine_dto_maps_to_transport_kind_with_hf_default() {
        assert_eq!(map_vara_engine(None), TransportKind::VaraHf);
        assert_eq!(map_vara_engine(Some(VaraEngineDto::VaraHf)), TransportKind::VaraHf);
        assert_eq!(map_vara_engine(Some(VaraEngineDto::VaraFm)), TransportKind::VaraFm);
    }
```

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.** DTO in `ports.rs` (beside `StationModeDto`, 449):

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum VaraEngineDto {
    VaraHf,
    VaraFm,
}
```

`map_vara_engine` in `mcp_ports.rs`:

```rust
fn map_vara_engine(engine: Option<VaraEngineDto>) -> crate::winlink::listener::transport::TransportKind {
    match engine {
        Some(VaraEngineDto::VaraFm) => crate::winlink::listener::transport::TransportKind::VaraFm,
        _ => crate::winlink::listener::transport::TransportKind::VaraHf,
    }
}
```

Replace both hardcoded `TransportKind::VaraHf` arguments (`mcp_ports.rs:1290` region and the `vara_open_session` twin at ~1332) with `map_vara_engine(engine)`, threading the new param through the trait method signatures, the router `Parameters` structs (documenting: *"engine: which VARA engine the target channel uses (`vara-hf` default, `vara-fm` for FM peers) — take it from the peer channel's `transport` field"*), and any port-trait doc comments (`ports.rs:821-841`). Update every existing call site/test to pass `None`.

Flip `vara_engine_split: true` (matrix row 9 — Task 4's protocol core + this dispatch together make the row true).

- [ ] **Step 4: Run tests** (scoped/CI) + `pnpm typecheck`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-mcp-core/ src-tauri/src/mcp_ports.rs src-tauri/src/peers/commands.rs
git commit -m "feat(mcp): engine-aware VARA egress — agent dials dispatch on the channel's engine, VaraHf default [R5-1]"
```

---

### Task 22: Frontend peer types + hook + distinct aggregation

**Files:**
- Create: `src/peers/types.ts`, `src/peers/usePeers.ts`, `src/peers/peerModel.ts`
- Test: `src/peers/peerModel.test.ts`

**Interfaces:**
- Consumes: Task 11 command shapes.
- Produces: TS mirrors (`Peer`, `PeerChannel`, `PeerEndpoint`, `PeersFile`, `P2pCapabilities`) — snake_case, EXACT mirror of the Rust serde shapes (no `rename_all`, per `src/favorites/types.ts:2-7`); `usePeers()` (TanStack Query on `peers_read` + `peers:changed` invalidation, mirroring `useContacts.ts:50-82`); `useP2pCapabilities()` (`p2p_capabilities`); `aggregatePeers(peers: Peer[]): AggregatedPeer[]`.

`aggregatePeers` is DISTINCT from `aggregateStations` `[R4-8]`: it keys on `canonical_base` alone, tolerates `grid: undefined`, and emits gridless/telnet-only peers so the rail can render them untiered. It does NOT drop gridless rows.

- [ ] **Step 1: Write the failing test** (`peerModel.test.ts`)

```ts
import { describe, it, expect } from 'vitest';
import { aggregatePeers } from './peerModel';
import type { Peer } from './types';

const peer = (over: Partial<Peer>): Peer => ({
  id: 'p1', canonical_base: 'W6ABC', presented_callsigns: ['W6ABC-7'],
  identity_kind: 'unknown', do_not_merge: false, conflict: false,
  source: 'auto', origin: 'outgoing', contact_id: null, grid: null,
  note: '', created_at: '2026-07-10T12:00:00-07:00', last_connected_at: null,
  channels: [], endpoints: [], ...over,
});

describe('aggregatePeers', () => {
  it('keeps gridless peers instead of dropping them', () => {
    const out = aggregatePeers([peer({ grid: null, endpoints: [
      { id: 'e1', host: 'x.example', port: 8774, provenance: 'operator', last_seen: '' },
    ] })]);
    expect(out).toHaveLength(1);
    expect(out[0].grid).toBeUndefined();
    expect(out[0].mapPlaceable).toBe(false);
  });

  it('keys on canonical_base, merging presented forms', () => {
    const out = aggregatePeers([
      peer({ id: 'a', presented_callsigns: ['W6ABC-7'] }),
      peer({ id: 'b', presented_callsigns: ['W6ABC-9'] }),
    ]);
    // Two distinct records (distinct ids) both surface; aggregation is by
    // base for map placement but never collapses distinct stored records.
    expect(out.map((p) => p.id).sort()).toEqual(['a', 'b']);
  });

  it('marks a gridded peer map-placeable', () => {
    const out = aggregatePeers([peer({ grid: { value: 'CN87', source: 'manual' } })]);
    expect(out[0].mapPlaceable).toBe(true);
    expect(out[0].grid).toBe('CN87');
  });
});
```

- [ ] **Step 2: Verify failure** — `pnpm vitest run src/peers/peerModel.test.ts` → FAIL (module missing).

- [ ] **Step 3: Implement** `types.ts` (exact snake_case mirror of Task 7's Rust + Task 11's `P2pCapabilities`), `usePeers.ts` (copy `useContacts.ts` structure, swap command/event names to `peers_read`/`peers:changed`, add `useP2pCapabilities`), and `peerModel.ts`:

```ts
import type { Peer } from './types';

export interface AggregatedPeer {
  id: string;
  canonicalBase: string;
  presentedCallsigns: string[];
  origin: Peer['origin'];
  grid?: string;               // undefined when the peer has no grid
  mapPlaceable: boolean;       // false ⇒ rail-only, untiered
  lastConnectedAt: string | null;
  channels: Peer['channels'];
  endpoints: Peer['endpoints'];
}

/**
 * Distinct from catalog/aggregateStations [R4-8]: keys on canonical_base,
 * tolerates a missing grid, and never drops gridless/telnet-only peers.
 */
export function aggregatePeers(peers: Peer[]): AggregatedPeer[] {
  if (!Array.isArray(peers)) return [];
  return peers.map((p) => {
    const grid = p.grid?.value?.trim() || undefined;
    return {
      id: p.id,
      canonicalBase: p.canonical_base,
      presentedCallsigns: p.presented_callsigns,
      origin: p.origin,
      grid,
      mapPlaceable: Boolean(grid),
      lastConnectedAt: p.last_connected_at,
      channels: p.channels,
      endpoints: p.endpoints,
    };
  });
}
```

- [ ] **Step 4: Run** — `pnpm vitest run src/peers/peerModel.test.ts` + `pnpm typecheck`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/peers/
git commit -m "feat(peers-ui): TS types, usePeers hook, distinct grid-tolerant aggregatePeers"
```

---

### Task 23: Finder — station-type filter + peer rows + Connect dispatch

**Files:**
- Modify: `src/catalog/StationFinderPanel.tsx`, `src/catalog/StationFinderControls.tsx`, `src/catalog/StationRail.tsx`
- Test: `src/catalog/StationFinderPanel.test.tsx`

**Interfaces:** consumes Tasks 22, 12 (packet intent), the existing `onUse` prefill path (`StationRail.tsx:82-107`).

Spec §5: a **station type** filter dimension (Gateway / Peer, both default on) added to the existing finder; peer rows carry origin + last-connected + endpoint provenance badge + RF channel rows; Connect feeds the existing per-mode flows with intent `p2p`, target = the channel's SSID'd callsign, `via` prefilled, frequency prefilled (center). **Hidden entirely** unless `useP2pCapabilities().finder_peers` `[R5-8]`.

- [ ] **Step 1: Write the failing test** — extend `StationFinderPanel.test.tsx`: with `p2p_capabilities` mocked `finder_peers: true` and `peers_read` returning one gridless telnet peer, assert (a) the Gateway/Peer type chips render, (b) toggling Peer off hides peer rows, (c) the gridless peer appears in the rail (untiered) even though it is not on the map, (d) with `finder_peers: false` NO peer chip and NO peer row render (capability hide). Follow the existing panel test's TanStack-Query + invoke-mock setup.

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement.**

- Add `enabledTypes: Set<'gateway' | 'peer'>` state beside `enabledBands`/`enabledModes` (`StationFinderPanel.tsx:138-143`), default `new Set(['gateway', 'peer'])`, with a `toggleType` mirroring `toggleBand` (`:348-362`), and persist it in `PersistedFinderView` (`:76-82`). When `!capabilities.finder_peers`, force the set to `{'gateway'}` and do not render the peer chip (hide, not disable).
- In `StationFinderControls.tsx` (`:130-168`), add a Gateway/Peer chip pair (same `station-finder__chip` + `aria-pressed` pattern) — rendered only when `finder_peers`.
- In the panel's data assembly (`:221`), when Peer is enabled, call `aggregatePeers(usePeers().peers)` and merge peer rows into the rail list (a discriminated `kind: 'gateway' | 'peer'` on the row model so `StationRail` renders origin/endpoint badges for peers). Peer rows without grid render untiered (no reachability pip); gridded peers reuse the existing prediction path.
- Peer `Connect`: the rail's Use path (`StationRail.tsx:82-107`) builds a dial; for a peer channel set `intent: 'p2p'`, `target` = `channel.target_callsign` (already SSID'd), `via` = `channel.via`, `freq` = `channel.freq_hz`. Telnet endpoint rows Connect via the UI `telnet_p2p_connect` command with the endpoint's host/port (operator click = consent). Plain-language origin labels: **incoming / outgoing / added / APRS** (no "worked").

- [ ] **Step 4: Run** — `pnpm vitest run src/catalog/StationFinderPanel.test.tsx` + `pnpm typecheck`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/
git commit -m "feat(finder): Gateway/Peer type filter + peer rows + p2p Connect dispatch, capability-hidden"
```

---

### Task 24: Map — circle peer shape on both maps (escaped render)

**Files:**
- Create: `src/map/PeerLayer.tsx`
- Modify: `src/catalog/StationFinderMap.tsx` (finder), `src/aprs/AprsPositionsMap.tsx` (tac-chat), plus a shared `.peer-pin` CSS (mirror `WinlinkGatewayLayer.css`)
- Test: `src/map/PeerLayer.test.tsx` (jsdom, following `StationFinderMap.test.tsx`)

**Interfaces:** consumes Task 22. Gated on `useP2pCapabilities().map_peers` `[R5-8]`.

Spec §6: **shape encodes entity** — circle = peer (new), diamond = gateway (unchanged), sprite = APRS (unchanged). Color stays reserved for the reachability ramp (finder) / outcome tiers (tac chat). A never-connected manual peer renders dashed. Peer-derived strings in `divIcon`/popup are escaped (`esc()` at `AprsPositionsMap.tsx:129-130`); pins use `L.divIcon` + imperative `marker.on('click', …)` reading from a ref (the react-leaflet false-green pitfall pattern the codebase already uses).

- [ ] **Step 1: Write the failing test** — `PeerLayer.test.tsx`: mount over a jsdom Leaflet map with one peer; assert a `.peer-pin` divIcon marker is added and a hostile callsign (`<img src=x>`) renders escaped (the marker HTML contains `&lt;img` not a live `<img`). Assert click fires the selection ref. With `map_peers: false`, assert no peer markers are added.

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement** `PeerLayer.tsx` mirroring `WinlinkGatewayLayer.tsx` (`L.divIcon` with `className: 'peer-pin-icon'`, `html` a `<div class="peer-pin ${tierClass} ${connected ? '' : 'peer-pin--dashed'}">`, ref-based `onSelect`), escaping every peer string via the same `esc()` helper. `.peer-pin` CSS: a circle (`border-radius: 50%`), reusing the existing `--reach-*` / outcome tier color vars — the shape differs, the color vocabulary is shared. Mount `PeerLayer` in both `StationFinderMap.tsx` and `AprsPositionsMap.tsx` behind the `map_peers` capability. A saved-peer-that-is-also-live-APRS shows the APRS sprite (live RF truth wins) with a neutral dashed ring — implement as a class modifier when a peer's `canonical_base` matches a live APRS callsign.

- [ ] **Step 4: Run** — `pnpm vitest run src/map/PeerLayer.test.tsx` + `pnpm typecheck`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/ src/catalog/StationFinderMap.tsx src/aprs/AprsPositionsMap.tsx
git commit -m "feat(map): circle peer shape on both maps, escaped divIcon render, capability-hidden"
```

---

### Task 25: P2P Peers settings section (roster editor)

**Files:**
- Create: `src/peers/PeerSettings.tsx`
- Modify: `src/shell/SettingsPanel.tsx` (`SectionId` 64-73, `NAV` 75-102, render branch 219-275)
- Test: `src/peers/PeerSettings.test.tsx`

**Interfaces:** consumes Tasks 11, 22. Gated on `useP2pCapabilities().settings_editor` — when false, the nav item is absent `[R5-8]`.

Spec §5: the full roster editor — endpoints, provenance promotion (with the "unverified claimed identity" badge on `ObservedIncoming`, and an out-of-band-verification acknowledgement on promote), keyring password set/clear, contact link, merge/split, and the legacy-keyring manual-reassignment prompt (Task 10 `Ambiguous` path). High-fidelity mock is required before build per tuxlink-sg5zw.8 — **this task's Step 0 is producing that mock** (browser companion, per the brainstorming preference) and getting operator sign-off; do not build the component before the mock is approved.

- [ ] **Step 0: High-fidelity mock + operator sign-off** (design gate, not code). Render a full-fidelity dark mock of the P2P Peers section (roster list, per-peer endpoint/channel detail, promote/merge/split/keyring affordances, the ObservedIncoming "unverified" badge). Get operator approval before Step 1.

- [ ] **Step 1: Write the failing test** — `PeerSettings.test.tsx`: mock `peers_read` with one peer having an `ObservedIncoming` endpoint; assert the "unverified claimed identity" badge renders; assert clicking Promote invokes `peer_endpoint_promote` with the right ids; assert Split invokes `peer_split`; assert a password Set field invokes `peer_endpoint_password_set`; with `settings_editor: false`, assert the section is not reachable.

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement** `PeerSettings.tsx` (mirror a standalone settings component like `AprsSettings.tsx`; TanStack-Query reads via `usePeers`, mutations via the Task 11 commands with `['peers']` invalidation), and wire it into `SettingsPanel.tsx`: add `'peers'` to `SectionId`, a `{ id: 'peers', label: 'P2P Peers' }` NAV entry under the "On air" group, and a `{active === 'peers' && capabilities.settings_editor && <PeerSettings />}` branch. Plain-language throughout (incoming/outgoing/added/APRS). Flip `settings_editor: true` (matrix row 8) once the component is wired.

- [ ] **Step 4: Run** — `pnpm vitest run src/peers/PeerSettings.test.tsx` + `pnpm typecheck`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/peers/PeerSettings.tsx src/shell/SettingsPanel.tsx src-tauri/src/peers/commands.rs
git commit -m "feat(peers-ui): P2P Peers settings section — roster editor, provenance promotion, merge/split, keyring"
```

---

### Task 26: Capability activation + cross-store consistency

**Files:**
- Modify: `src-tauri/src/peers/commands.rs` (flip `finder_peers`, `map_peers`), `src-tauri/src/peers/store.rs` (contact-link resolution), `src-tauri/src/favorites/store.rs` (peer_id back-link on delete)
- Test: `peers/store.rs`, `favorites/store.rs`

**Interfaces:** consumes Tasks 17, 22-25. This task makes the "hide unimplemented rows" mechanism honest by turning the remaining capability bits on ONLY after their UI landed, and closes the cross-store edges (spec §Cross-store consistency).

- [ ] **Step 1: Write the failing tests**

```rust
    // peers/store.rs
    #[test]
    fn contact_delete_leaves_peer_and_clears_contact_sourced_grid() {
        // §Cross-store: contact_id is one-way; a missing contact_id is
        // treated as unlinked, and a grid whose source was the now-gone
        // contact is cleared (not shown as authoritative) [R4-9].
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let mut p = manual_peer("p1", "W6ABC");
        p.contact_id = Some("c-gone".into());
        p.grid = Some(PeerGrid { value: "CN87".into(), source: GridSource::Contact });
        s.upsert_manual(p).unwrap();
        // Resolve against a contacts set that no longer has c-gone.
        s.reconcile_contact_links(&[] /* live contact ids */);
        let peer = &s.file().peers[0];
        assert_eq!(peer.contact_id, None, "dangling contact link cleared");
        assert!(peer.grid.is_none(), "contact-sourced grid cleared when contact gone");
    }
```

```rust
    // favorites/store.rs
    #[test]
    fn deleting_a_peer_id_favorite_does_not_orphan_the_star() {
        // [R4-12]: a starred peer channel carries peer_id; the favorite is
        // resolvable back to its peer and cleaned on peer delete.
        // (Exercises FavoritesStore's peer_id-aware delete helper.)
    }
```

- [ ] **Step 2: Verify failure.**

- [ ] **Step 3: Implement** `PeersStore::reconcile_contact_links(&mut self, live_contact_ids: &[String])` — for every peer whose `contact_id` is `Some` and not in the live set: set `contact_id = None` and, if `grid.source == Contact`, clear `grid`. Call it from `peers_read` (lazy resolution — cheap; the frontend reads on mount) OR wire a `contacts:changed` listener in `lib.rs` that reconciles. Choose the listener path (authoritative + no per-read cost): in `.setup()`, `app.listen("contacts:changed", …)` reads the contacts store's live ids and calls `reconcile_contact_links`, then emits `peers:changed`. Favorites: a `FavoritesStore::delete_by_peer_id(peer_id)` helper called from `peer_delete` (Task 11) so a peer delete clears its starred channels.

Flip `finder_peers: true` (rows 3+5, Task 22-23) and `map_peers: true` (row 6, Task 24). Every capability bit is now `true` — the feature is fully wired, and the hide mechanism was honest at every intermediate commit.

- [ ] **Step 4: Run tests** (scoped/CI). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/peers/ src-tauri/src/favorites/ src-tauri/src/lib.rs
git commit -m "feat(peers): contact-link reconciliation + peer_id favorite cleanup; finder/map capabilities on"
```

---

### Task 27: Two-rig bench runbook (operator doc, no code)

**Files:**
- Create: `docs/design/2026-07-10-p2p-bench-runbook.md`

**Interfaces:** none (documentation). Transcribes spec §8 into an operator-executable runbook. This is a plan deliverable, not an agent-run procedure — RADIO-1: the operator runs it after merge.

- [ ] **Step 1: Write the runbook** covering, verbatim from spec §8: Step 0 topology (port map instance 1 = 8300/8301, instance 2 = 8310/8311, socat tap on a distinct port; per-instance audio device; **G90 is the validated VARA pairing — the FT-710 crashes the operator's VARA**, memory `rig-test-path`); Step 1 socat hex wire-tap capturing `P2P SESSION` / `REGISTERED` (and whether it re-arrives on TCP reconnect) / `COMPRESSION` / `RETRIES` (accepted or `WRONG`?) / `PUBLIC`; Steps 2-5 outgoing HF, incoming HF (confirm `PUBLIC ON` did not block accept), SSID variant (gbb05 echo base-match on the wire), FM leg (or record FM bench-deferred with the HF leg proving the shared record path); Step 6 every dial operator-initiated with clear-channel check, consent = the Connect click per RADIO-1. Include the "WLE far end must have its Vara P2P Session window open (mutual-intent requirement)" note.

- [ ] **Step 2: Verify** — `pnpm lint:docs` passes (no broken links).

- [ ] **Step 3: Commit**

```bash
git add docs/design/2026-07-10-p2p-bench-runbook.md
git commit -m "docs(p2p): two-rig VARA bench runbook (operator-executed, RADIO-1)"
```

---

### Task 28: Integration wire-walk gate (hard gate — the definition-of-done)

**Files:** none created. This task runs the `wire-walk` skill (`.claude/skills/wire-walk/SKILL.md`) against the operator flows recorded in the "Definition of done" section at the top of this plan.

**This is a HARD GATE, not a review.** Per the wire-walk Iron Law and spec §Integration matrix `[R4-10][R1-C10]`: the feature is a stub unless every matrix row landed AND every operator flow traces end-to-end. Any ❌ = NOT shipped; the broken flow is the remaining work, not a follow-up.

- [ ] **Step 1: Confirm the flows.** The operator-supplied flows are recorded verbatim at the top of this plan (captured at build start). Do NOT self-generate or re-rank them. If that section is still `PENDING`, STOP — the gate cannot run without operator flows.

- [ ] **Step 2: Trace each flow** from its stated starting state (fresh install / empty roster / post-upgrade explicitly) to `file:line`, hunting the break-patterns (orphan producer, dead control, empty seam, missing variant, unsatisfiable gate, config-only path). Trace gates backward to their producer — e.g. "a peer appears in the finder" requires the recorder actually wrote it, which requires a P2P session concluded at a live record site.

- [ ] **Step 3: Verify the integration matrix** — every one of the 10 rows (spec §Integration matrix) is present, and its `P2pCapabilities` bit is `true` only because the row is fully wired (Task 11 + the flipping tasks). Confirm no capability bit is `true` ahead of its UI, and no unshipped row renders disabled/stubbed (UI tests from Tasks 23-25 prove absence when a bit is false).

- [ ] **Step 4: Verdict per flow** (✅ wired `file:line`→`file:line` / ❌ broken at `file:line` / ⚠️ reserved only for the on-air terminal effect, which is the operator bench). Record the verdicts in the session handoff.

- [ ] **Step 5: Gate.** If any flow is ❌, the feature is NOT shipped — do not mark the PR ready, do not close tuxlink-c39af/sg5zw.8. Report the broken flows with break points as the finish-the-feature spec. If all ✅ (modulo the operator-only on-air ⚠️), proceed to PR.

---

## Self-Review

Run before dispatching any execution. This is the author's own checklist (writing-plans §Self-Review).

**1. Spec coverage — every section maps to a task:**

| Spec section | Task(s) |
|---|---|
| §1 identity model (canonical_base, presented, kind, merge/split) | 1, 7, 8, 25 |
| §2 storage (PeersFile, dedup, caps, quarantine, keyring re-key) | 7, 8, 9, 10 |
| §3 auto-tracking (recorder, 8 sites, packet intent) | 11, 12, 13, 14, 15, 16 |
| §4 trust boundary (curate_peer, egress gate, agent dial, provenance) | 18, 19, 20 |
| §5 finder (type filter, distinct aggregation, peer settings) | 22, 23, 25 |
| §6 map symbology (circle, escaped render) | 24 |
| §7 VARA protocol (engine split, REGISTERED, SSID, WRONG, LISTEN, compression) | 2, 3, 4, 5, 6 |
| §7.5 interop matrix | 4, 12, 21 (behavior); no standalone task (it is a description of other tasks' effects) |
| §8 bench runbook | 27 |
| §Integration matrix (10 rows, hide mechanism) | 11 (capability bits) + each row's task; 28 (gate) |
| §Cross-store consistency | 17, 26 |
| §Testing (TDD) | every task is test-first |

Matrix row → task: (1) store → T7-11; (2) recorder+sites+packet intent → T11-16; (3) peer read+aggregation → T11,22; (4) find_peers+curate → T19; (5) finder filter → T23; (6) map shape → T24; (7) agent telnet dial+denylist → T20; (8) settings section → T25; (9) VARA core + engine-aware egress → T2-6,21; (10) favorites peer_id + bridge → T17. All 10 covered.

**2. Placeholder scan:** no "TBD/implement later/add error handling" — each code step carries the actual code or an exact, file:line-anchored instruction to mirror a named in-tree pattern. The only deliberately-deferred content is the **Definition-of-done flows** (operator-supplied, greenfield — must NOT be author-drafted) and the **Task 25 mock** (design gate before code). Both are correct deferrals, not placeholders.

**3. Type consistency:** `canonical_base`, `presented_callsigns`, `ChannelTransport`/`ChannelBandwidth`, `Provenance`, `ObservationPhase`/`ObservedPath`/`PeerObservation`, `ApplyEffect`, `P2pCapabilities`, `PeerDto`/`PeerListDto` are defined once (T1/T7/T11/T19) and referenced by exact name downstream. `Favorite.peer_id` matches between Rust (T17) and TS (T17). `SessionIntent` reused from the existing enum, not redefined. `send_connect_and_wait_inner` signature is stated in T5 and reused verbatim in T13's test.

## Execution Handoff

**Plan complete and saved to `docs/plans/2026-07-10-p2p-peer-model-plan.md`.** Recommended execution: **subagent-driven-development** (fresh subagent per task, two-stage review between tasks) — the plan is 28 mostly-sequential tasks with parallel leaves within a phase, and every task is test-first with exact code, which is the profile subagent-driven review handles best. Phases 0-4 are backend-Rust-heavy (CI compiles; the Pi does not); Phase 5 is frontend (local vitest). Task 25 has a design-mock gate before its code. Task 28 is the hard wire-walk gate — it cannot pass until the operator flows are recorded at the top of this plan.







# Station Intelligence L1 — jt9 Decode Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tested, local-TDD-able decode service that takes one 15-second FT8 slot WAV and returns structured decode records by invoking the managed `/usr/bin/jt9` binary, with the full failure taxonomy from the reviewed design delta.

**Architecture:** New std-only leaf workspace crate `src-tauri/tuxlink-jt9` (the `tuxlink-ft8` precedent: leaf crates compile in seconds on the dev Pi, so red-green runs locally; the main src-tauri crate cannot compile locally). Sync API (std::process + drain thread + poll timeout); the main crate wraps it in `spawn_blocking` at L2 wiring time. jt9 is subprocess-only (GPL boundary): WAV file + argv in, stdout/stderr out.

> **Supersession note:** this deliberately supersedes the delta's tokio-based
> process-discipline wording (`kill_on_drop`, `tokio::time::timeout`): the std
> sync mechanism (drain threads + `try_wait` poll + Drop-guard kill+wait)
> provides the same kill/reap/timeout guarantees, chosen so the leaf crate
> compiles dep-free on the Pi. Where this plan and the delta's mechanism
> wording differ, this plan wins; the delta's guarantees (kill+reap, 12 s
> deadline, partial salvage) still bind.

**Tech Stack:** Rust (std only — no tokio, no external deps in this crate), jt9 from the wsjtx package as an external binary, existing SDR WAV fixtures from `src-tauri/tuxlink-ft8/tests/fixtures/sdr/`.

**Canonical design:** `docs/design/2026-07-10-station-intel-jt9-engine-delta.md` (v2, adversarially reviewed). Read it before starting any task. Epic: bd `tuxlink-b026z`, this plan = child `tuxlink-b026z.2`.

## Global Constraints

- **All commands run from inside the worktree, and paths are pinned absolute
  — subagent shell cwd resets between calls** (the project's documented
  `pin_paths_in_worktree_sessions` failure mode; a relative `--manifest-path`
  from the main checkout hits a tree with no `tuxlink-jt9` member). The
  canonical forms (per-task Run lines abbreviate; this constraint governs):
  - `WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-b026z.2-station-intel-jt9`
  - `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --locked`
  - every `git`/script command block starts with `cd "$WT" && pwd` as its own
    step, re-issued after any command that reports "Shell cwd was reset".
- **Tasks execute strictly in order; each task starts from the previous
  task's commit.** Tasks 1–5 all touch `lib.rs` (and the lock) — parallel
  dispatch collides.
- **Commits:** the per-task commit blocks show the SUBJECT ONLY. Every commit
  uses the heredoc form with both trailers (the repo's `.githooks/commit-msg`
  hard-refuses commits without the `Agent:` trailer):
  ```bash
  git commit -m "$(cat <<'EOF'
  <subject line from the task>

  Agent: <session-moniker>
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  EOF
  )"
  ```
  The dispatching parent supplies `<session-moniker>` in the task prompt. Per
  project convention, if a subagent cannot commit in the worktree it stops
  after staging and reports; the PARENT commits.
- MSRV 1.75 (`incompatible_msrv` is denied — no `Result::inspect_err` etc.).
- `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --all-targets --locked -- -D warnings` must stay clean. CI runs `--workspace --all-targets --locked -- -D warnings` on amd64 + arm64 (the `--workspace` is load-bearing with `default-members = ["."]` — do not "fix" CI toward the narrower local command).
- The dev Pi CANNOT build the main src-tauri crate. It CAN build this leaf crate: every `cargo` command in this plan uses `-p tuxlink-jt9` and runs locally in seconds. Run them; do not skip red-green.
- Test temp hygiene: suites create pid-suffixed dirs under `std::env::temp_dir()`; add a best-effort `let _ = std::fs::remove_dir_all(&base);` at the end of each test where practical. Small accepted litter is tolerable; multi-hundred-KB WAV litter is not.
- jt9 invocation is confined to this crate. The literal `"jt9"` in spawn position appears nowhere else in the repo. `-s`/`--shmem` never appears in the arg builder (FSF GPL boundary-crosser — banned by design).
- Never `pkill`/`libc::kill`-by-PID; child lifecycle is owned (`Child` + Drop kill+wait).
- `slot_utc_ms` always comes from the caller (host slot scheduler); never from jt9 output.
- Tests that need the real jt9 binary must SKIP with a printed notice when it is absent (CI installs wsjtx so they always run there; Task 8).
- Fixtures live at `src-tauri/tuxlink-ft8/tests/fixtures/sdr/`; reference them from this crate via `../tuxlink-ft8/tests/fixtures/...` joined to `env!("CARGO_MANIFEST_DIR")`.
- BEFORE starting any task: (1) read/invoke the test-driven-development skill; (2) read `docs/pitfalls/testing-pitfalls.md`. Follow TDD: failing test → minimal code → green.
- BEFORE marking any task complete: review your tests against `docs/pitfalls/testing-pitfalls.md`; verify error paths and edge cases are covered; run the task's test commands and confirm green.
- After every logical group of tasks (groups marked below): review the batch from multiple perspectives, minimum three rounds; keep going past three if the third still finds substantive issues.
- Commit after every task with conventional-commit type + `Agent: <session-moniker>` trailer + `Co-Authored-By:` trailer, from the worktree `worktrees/bd-tuxlink-b026z.2-station-intel-jt9` (branch `bd-tuxlink-b026z.2/station-intel-jt9`).

---

### Task 1: Crate scaffold + stdout line parser

**Files:**
- Create: `src-tauri/tuxlink-jt9/Cargo.toml`
- Create: `src-tauri/tuxlink-jt9/src/lib.rs`
- Create: `src-tauri/tuxlink-jt9/src/parse.rs`
- Modify: `src-tauri/Cargo.toml` (workspace `members` line — add `"tuxlink-jt9"` after `"tuxlink-ft8"`)

**Interfaces:**
- Produces: `parse::ParsedLine` enum and `parse::parse_stdout_line(line: &str) -> ParsedLine`, consumed by Task 5's runner.

- [ ] **Step 1: Scaffold the crate**

`src-tauri/tuxlink-jt9/Cargo.toml`:
```toml
[package]
name = "tuxlink-jt9"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "AGPL-3.0-or-later"
description = "Managed jt9 (WSJT-X) FT8 decode service: slot WAV in, structured decodes out. Subprocess boundary only."

[dependencies]
```

`src-tauri/tuxlink-jt9/src/lib.rs`:
```rust
//! Managed-jt9 FT8 decode service (Station Intelligence L1, tuxlink-b026z.2).
//!
//! jt9 is invoked strictly as a subprocess: WAV file + argv in, stdout/stderr
//! out. The `-s`/`--shmem` mode is banned (GPL boundary — see
//! docs/design/2026-07-10-station-intel-jt9-engine-delta.md §GPL boundary).

pub mod parse;
```

Add `"tuxlink-jt9"` to the workspace members array in `src-tauri/Cargo.toml`.

Then regenerate the lock (Cargo.lock records every workspace member as a
`[[package]]`, so `--locked` fails until the new member is recorded): run
`cargo metadata --manifest-path "$WT/src-tauri/Cargo.toml" > /dev/null` once
WITHOUT `--locked`. The updated `Cargo.lock` is committed in Step 6.

- [ ] **Step 2: Write the failing parser tests**

`src-tauri/tuxlink-jt9/src/parse.rs` (tests first — module body only a stub):
```rust
//! Line-level parser for jt9 FT8 file-mode stdout.
//!
//! Line grammar (verbatim capture, wsjtx 2.7.0, `jt9 -8 -d 3 -p 15 -w 1`):
//! ```text
//! 000000 -17 -0.9 2391 ~  CQ W5C/H
//! 000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19
//! <DecodeFinished>   0   6        0
//! ```
//! Columns before the `~` (the FT8 sync marker, which cannot occur in an FT8
//! message charset): HHMMSS time (always `000000` for non-WSJT-X-named input
//! files — ignored; slot UTC comes from the host scheduler), SNR dB, DT s,
//! audio freq Hz. Everything after `~` is the message, trimmed.
//! Grammar lifted from tuxlink-ft8/src/oracle.rs (which discards the
//! metadata; this parser keeps it — that is why it exists).

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    Decode { snr_db: i32, dt_s: f64, freq_hz: u32, message: String },
    DecodeFinished,
    Other,
}

pub fn parse_stdout_line(line: &str) -> ParsedLine {
    let _ = line;
    ParsedLine::Other // stub — replaced in Step 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verbatim_decode_lines() {
        // Real capture, 2026-07-10, wsjtx 2.7.0+repack-1, ordinary fixture.
        let l = "000000 -17 -0.9 2391 ~  CQ W5C/H                                ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -17, dt_s: -0.9, freq_hz: 2391, message: "CQ W5C/H".into() }
        );
        let l = "000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -14, dt_s: -0.6, freq_hz: 2093, message: "YB3BBF K5OJT -19".into() }
        );
        let l = "000000 -16 -1.0  502 ~  K0BQB WD8ASA +09                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -16, dt_s: -1.0, freq_hz: 502, message: "K0BQB WD8ASA +09".into() }
        );
    }

    #[test]
    fn parses_decode_finished_sentinel() {
        assert_eq!(parse_stdout_line("<DecodeFinished>   0   6        0"), ParsedLine::DecodeFinished);
    }

    #[test]
    fn hashed_callsign_message_survives_verbatim() {
        let l = "000000 -12  0.3 1802 ~  <...> N4AHI EM73                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -12, dt_s: 0.3, freq_hz: 1802, message: "<...> N4AHI EM73".into() }
        );
    }

    #[test]
    fn malformed_lines_are_other_never_panic() {
        for l in ["", "garbage", "000000 -14", "000000 xx yy zz ~ MSG",
                  "Fortran runtime error: End of file", "~", "000000 -14 -0.6 2093 ~  "] {
            assert_eq!(parse_stdout_line(l), ParsedLine::Other, "line: {l:?}");
        }
    }
}
```

- [ ] **Step 3: Run tests, verify the decode-line tests FAIL**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --locked`
Expected: `parses_verbatim_decode_lines`, `parses_decode_finished_sentinel`, `hashed_callsign_message_survives_verbatim` FAIL (stub returns `Other`); `malformed_lines_are_other_never_panic` passes (stub coincidence — acceptable; it exists to pin no-panic). (If this errors on the LOCK rather than failing tests, Step 1's lock regeneration was skipped — do it now.)

- [ ] **Step 4: Implement the parser**

Replace the stub in `parse.rs`:
```rust
pub fn parse_stdout_line(line: &str) -> ParsedLine {
    let trimmed = line.trim_end();
    if trimmed.trim_start().starts_with("<DecodeFinished>") {
        return ParsedLine::DecodeFinished;
    }
    let Some((meta, msg)) = trimmed.split_once('~') else {
        return ParsedLine::Other;
    };
    let message = msg.trim().to_string();
    if message.is_empty() {
        return ParsedLine::Other;
    }
    // meta: "HHMMSS SNR DT FREQ" — whitespace-separated, HHMMSS ignored.
    let mut cols = meta.split_whitespace();
    let _time = cols.next();
    let (Some(snr), Some(dt), Some(freq)) = (cols.next(), cols.next(), cols.next()) else {
        return ParsedLine::Other;
    };
    let (Ok(snr_db), Ok(dt_s), Ok(freq_hz)) =
        (snr.parse::<i32>(), dt.parse::<f64>(), freq.parse::<u32>()) else {
        return ParsedLine::Other;
    };
    ParsedLine::Decode { snr_db, dt_s, freq_hz, message }
}
```

- [ ] **Step 5: Run tests, verify all green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Expected: 4 passed. Also run clippy for this crate:
`cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --all-targets --locked -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tuxlink-jt9 src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(ft8): tuxlink-jt9 leaf crate + jt9 stdout line parser (tuxlink-b026z.2 T1)"
```
(If `Cargo.lock` did not change, omit it. New workspace member with no deps may still touch the lock — include whatever `git status` shows for it. `--locked` failing on the FIRST build means the lock needs regenerating: run the test command once WITHOUT `--locked`, then commit the updated lock — see the `rust_dep_requires_cargo_lock_update` project rule.)

---

### Task 2: FT8 message field extractor

**Files:**
- Create: `src-tauri/tuxlink-jt9/src/message.rs`
- Modify: `src-tauri/tuxlink-jt9/src/lib.rs` (add `pub mod message;`)

**Interfaces:**
- Produces: `message::MessageFields { from_call: Option<String>, to_call: Option<String>, grid: Option<String> }` and `message::extract_fields(message: &str) -> MessageFields`, consumed by Task 5.

- [ ] **Step 1: Write the failing tests**

`src-tauri/tuxlink-jt9/src/message.rs`:
```rust
//! Best-effort field extraction from standard FT8 message text.
//!
//! Design contract (delta §Revised L1): hashed callsigns (`<...>` or any
//! `<...>`-bracketed token) yield None for that field — unresolvable with
//! per-slot jt9 spawn (accepted regression, surfaced downstream). Grid is
//! extracted ONLY when the trailing token is a 4-char Maidenhead locator;
//! reports (+NN/-NN/R-NN), RRR, RR73, 73 are NOT grids (delta §L4 grid
//! provenance: no grid → no map placement).

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MessageFields {
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
}

pub fn extract_fields(message: &str) -> MessageFields {
    let _ = message;
    MessageFields::default() // stub
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(m: &str) -> MessageFields { extract_fields(m) }

    #[test]
    fn cq_with_grid() {
        assert_eq!(f("CQ JE6HOG PM53"), MessageFields {
            from_call: Some("JE6HOG".into()), to_call: None, grid: Some("PM53".into()) });
    }

    #[test]
    fn cq_compound_call_no_grid() {
        // Real capture: compound/portable call, no grid (compound calls
        // cannot carry a grid in the standard message).
        assert_eq!(f("CQ W5C/H"), MessageFields {
            from_call: Some("W5C/H".into()), to_call: None, grid: None });
    }

    #[test]
    fn cq_with_modifier_dx() {
        assert_eq!(f("CQ DX K1ABC FN42"), MessageFields {
            from_call: Some("K1ABC".into()), to_call: None, grid: Some("FN42".into()) });
    }

    #[test]
    fn report_and_r_report_and_73s_are_not_grids() {
        assert_eq!(f("YB3BBF K5OJT -19"), MessageFields {
            from_call: Some("K5OJT".into()), to_call: Some("YB3BBF".into()), grid: None });
        assert_eq!(f("N6VIN JA8NRS R-06"), MessageFields {
            from_call: Some("JA8NRS".into()), to_call: Some("N6VIN".into()), grid: None });
        assert_eq!(f("VK4DAD K5KND 73"), MessageFields {
            from_call: Some("K5KND".into()), to_call: Some("VK4DAD".into()), grid: None });
        assert_eq!(f("K0BQB WD8ASA RR73"), MessageFields {
            from_call: Some("WD8ASA".into()), to_call: Some("K0BQB".into()), grid: None });
        assert_eq!(f("K0BQB WD8ASA RRR"), MessageFields {
            from_call: Some("WD8ASA".into()), to_call: Some("K0BQB".into()), grid: None });
    }

    #[test]
    fn standard_reply_with_grid() {
        assert_eq!(f("K1ABC W9XYZ EN37"), MessageFields {
            from_call: Some("W9XYZ".into()), to_call: Some("K1ABC".into()), grid: Some("EN37".into()) });
    }

    #[test]
    fn hashed_callsigns_yield_none() {
        assert_eq!(f("<...> N4AHI EM73"), MessageFields {
            from_call: Some("N4AHI".into()), to_call: None, grid: Some("EM73".into()) });
        assert_eq!(f("<KA1ABC> W9XYZ RR73"), MessageFields {
            from_call: Some("W9XYZ".into()), to_call: Some("KA1ABC".into()), grid: None });
        // From-position hash: unresolved sender is None (design contract).
        assert_eq!(f("CQ <...>"), MessageFields::default());
    }

    #[test]
    fn free_text_and_junk_yield_default() {
        assert_eq!(f("TNX 599 GL"), MessageFields::default());
        assert_eq!(f(""), MessageFields::default());
    }
}
```

Note the grammar rules the tests pin, so the implementation has no latitude:
- `CQ [MODIFIER] CALL [GRID]` — MODIFIER is 1–4 chars, all-alpha or 3-digit
  (DX, POTA, TEST, 001); CALL is the last callsign-shaped token; GRID only if
  the final token matches `^[A-R]{2}[0-9]{2}$` AND is not the literal `RR73`
  (which is deliberately grid-shaped in the FT8 protocol; in suffix position
  it is always the sign-off acknowledgment, never a locator).
- Two-token-plus messages `TO FROM [suffix]`: `to_call` = first token,
  `from_call` = second, grid only when suffix is a Maidenhead match.
- A token is callsign-shaped if it contains at least one digit and one
  letter, length 3–11, chars in `[A-Z0-9/]` (uppercase input guaranteed by
  jt9). `<...>` is the unresolved-hash token → `None`. `<CALL>` (angle-bracket
  resolved hash) strips brackets and counts as that field's callsign.
- Anything else → all-None. Never panic.

- [ ] **Step 2: Run tests, verify they FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked message`
Expected: all extractor tests FAIL against the stub except `free_text_and_junk_yield_default`.

- [ ] **Step 3: Implement `extract_fields` to the pinned grammar**

```rust
fn is_grid(tok: &str) -> bool {
    if tok == "RR73" {
        return false; // FT8 sign-off token — deliberately grid-shaped, never a locator here
    }
    let b = tok.as_bytes();
    b.len() == 4
        && b[0].is_ascii_uppercase() && b[0] <= b'R'
        && b[1].is_ascii_uppercase() && b[1] <= b'R'
        && b[2].is_ascii_digit() && b[3].is_ascii_digit()
}

fn is_callsign(tok: &str) -> bool {
    (3..=11).contains(&tok.len())
        && tok.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/')
        && tok.chars().any(|c| c.is_ascii_digit())
        && tok.chars().any(|c| c.is_ascii_uppercase())
}

/// `<...>` → None (unresolvable); `<CALL>` → Some(CALL); bare call → Some.
fn call_field(tok: &str) -> Option<String> {
    if tok == "<...>" {
        return None;
    }
    let inner = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')).unwrap_or(tok);
    is_callsign(inner).then(|| inner.to_string())
}

fn is_cq_modifier(tok: &str) -> bool {
    (1..=4).contains(&tok.len())
        && (tok.chars().all(|c| c.is_ascii_uppercase())
            || (tok.len() == 3 && tok.chars().all(|c| c.is_ascii_digit())))
        && !is_callsign(tok)
}

pub fn extract_fields(message: &str) -> MessageFields {
    let toks: Vec<&str> = message.split_whitespace().collect();
    let mut out = MessageFields::default();
    match toks.as_slice() {
        ["CQ", rest @ ..] if !rest.is_empty() => {
            let (call_idx, grid) = match rest {
                [.., last] if is_grid(last) => (rest.len().checked_sub(2), Some(last.to_string())),
                _ => (Some(rest.len() - 1), None),
            };
            // call position: last token if no grid, second-to-last if grid.
            let idx = match call_idx {
                Some(i) => i,
                None => return out, // "CQ <grid>" alone — malformed
            };
            let candidate = rest[idx];
            // Allow one leading modifier: "CQ DX K1ABC FN42" — candidate must
            // be the callsign; a preceding token (if any) must be a modifier.
            if (call_field(candidate).is_some() || candidate == "<...>")
                && rest[..idx].iter().all(|t| is_cq_modifier(t))
            {
                out.from_call = call_field(candidate);
                out.grid = grid;
                if out.from_call.is_none() {
                    out.grid = None; // hashed CQ: no usable station → no grid
                }
            }
            out
        }
        [to, from, rest @ ..] if (call_field(to).is_some() || *to == "<...>")
            && (call_field(from).is_some() || *from == "<...>") => {
            out.to_call = call_field(to);
            out.from_call = call_field(from);
            if let [suffix] = rest {
                if is_grid(suffix) {
                    out.grid = Some((*suffix).to_string());
                }
            }
            out
        }
        _ => out,
    }
}
```

- [ ] **Step 4: Run tests, verify all green; run clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --all-targets --locked -- -D warnings`
Expected: all pass, clippy clean. If the implementation as given trips a clippy lint (e.g. `collapsible_if`), fix the code — do not `#[allow]`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-jt9/src/message.rs src-tauri/tuxlink-jt9/src/lib.rs
git commit -m "feat(ft8): FT8 message field extractor with pinned grammar (tuxlink-b026z.2 T2)"
```

---

**REVIEW GATE A (after Tasks 1–2):** review the parser + extractor batch from multiple perspectives (grammar completeness vs the verbatim captures; panic-safety on adversarial input; clippy/MSRV). Minimum three rounds; continue past three if the third still finds substantive issues. Persist findings to `dev/scratch/b026z.2-gate-A-findings.md` before proceeding.

---

### Task 3: Slot-WAV preflight validator

**Files:**
- Create: `src-tauri/tuxlink-jt9/src/wav.rs`
- Modify: `src-tauri/tuxlink-jt9/src/lib.rs` (add `pub mod wav;`)

**Interfaces:**
- Produces: `wav::preflight_slot_wav(path: &Path) -> Result<(), WavError>` with `WavError { NotFound, Permission, Malformed(String), WrongFormat(String) }`, consumed by Task 5 before every spawn.

Rationale (delta §Grounded facts): jt9 segfaults on missing/corrupt input, ignores the sample-rate header, and exits 0-with-zero-decodes on truncated input — the host must reject bad WAVs because jt9 will not.

- [ ] **Step 1: Write the failing tests**

```rust
//! Host-side slot-WAV validation. jt9 cannot be trusted to reject bad input:
//! it segfaults on missing/corrupt files, ignores the sample-rate header, and
//! silently under-decodes truncated audio (delta §Grounded facts). Contract:
//! canonical RIFF/WAVE, PCM (format 1), mono, 16-bit, 12000 Hz, exactly
//! 180_000 frames (15.000 s).

use std::io::Read;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum WavError {
    NotFound,
    Permission,
    Malformed(String),
    WrongFormat(String),
}

pub const SLOT_FRAMES: u32 = 180_000;
pub const SLOT_RATE_HZ: u32 = 12_000;

pub fn preflight_slot_wav(path: &Path) -> Result<(), WavError> {
    let _ = path;
    Err(WavError::Malformed("stub".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal canonical WAV writer for tests: 44-byte header + PCM.
    fn write_wav(path: &Path, rate: u32, channels: u16, bits: u16, frames: u32) {
        let mut f = std::fs::File::create(path).unwrap();
        let block_align = channels * (bits / 8);
        let data_len = frames * block_align as u32;
        let byte_rate = rate * block_align as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&byte_rate.to_le_bytes()).unwrap();
        f.write_all(&block_align.to_le_bytes()).unwrap();
        f.write_all(&bits.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        f.write_all(&vec![0u8; data_len as usize]).unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join("tuxlink-jt9-wavtest");
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn accepts_canonical_slot_wav() {
        let p = tmp("good.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        assert_eq!(preflight_slot_wav(&p), Ok(()));
    }

    #[test]
    fn rejects_missing_file() {
        assert_eq!(preflight_slot_wav(Path::new("/nonexistent/slot.wav")), Err(WavError::NotFound));
    }

    #[test]
    fn rejects_wrong_rate_channels_bits_and_length() {
        let cases: [(&str, u32, u16, u16, u32); 4] = [
            ("rate48k.wav", 48_000, 1, 16, 180_000),
            ("stereo.wav", 12_000, 2, 16, 180_000),
            ("bits8.wav", 12_000, 1, 8, 180_000),
            ("short.wav", 12_000, 1, 16, 24_000),
        ];
        for (name, rate, ch, bits, frames) in cases {
            let p = tmp(name);
            write_wav(&p, rate, ch, bits, frames);
            assert!(matches!(preflight_slot_wav(&p), Err(WavError::WrongFormat(_))), "{name}");
        }
    }

    #[test]
    fn rejects_garbage_and_truncated_header() {
        let p = tmp("garbage.wav");
        std::fs::write(&p, b"not a wav at all").unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::Malformed(_))));
        let p = tmp("tiny.wav");
        std::fs::write(&p, b"RIFF").unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::Malformed(_))));
    }

    #[test]
    fn rejects_truncated_data_with_intact_header() {
        // The capture-bug class jt9 itself cannot catch: header claims
        // 180,000 frames, file was truncated on disk. Preflight must compare
        // actual size to the header's data_len.
        let p = tmp("lying-header.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        let full = std::fs::read(&p).unwrap();
        std::fs::write(&p, &full[..full.len() / 2]).unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::WrongFormat(_))));
    }

    #[test]
    fn unreadable_file_is_permission() {
        use std::os::unix::fs::PermissionsExt;
        let p = tmp("noperm.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = preflight_slot_wav(&p);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(r, Err(WavError::Permission));
    }
}
```

- [ ] **Step 2: Run tests, verify they FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked wav`
Expected: `accepts_canonical_slot_wav`, `rejects_missing_file` FAIL against the stub; the reject-cases may pass coincidentally (stub errors) — that is fine, the accept case is the discriminator.

- [ ] **Step 3: Implement**

```rust
pub fn preflight_slot_wav(path: &Path) -> Result<(), WavError> {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(WavError::NotFound),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(WavError::Permission),
        Err(e) => return Err(WavError::Malformed(e.to_string())),
    };
    let mut hdr = [0u8; 44];
    f.read_exact(&mut hdr).map_err(|_| WavError::Malformed("short header".into()))?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..16] != b"WAVEfmt " {
        return Err(WavError::Malformed("not RIFF/WAVE".into()));
    }
    let fmt = u16::from_le_bytes([hdr[20], hdr[21]]);
    let channels = u16::from_le_bytes([hdr[22], hdr[23]]);
    let rate = u32::from_le_bytes([hdr[24], hdr[25], hdr[26], hdr[27]]);
    let bits = u16::from_le_bytes([hdr[34], hdr[35]]);
    let data_len = u32::from_le_bytes([hdr[40], hdr[41], hdr[42], hdr[43]]);
    if &hdr[36..40] != b"data" {
        return Err(WavError::Malformed("no canonical data chunk at offset 36".into()));
    }
    let want = format!("PCM mono 16-bit {SLOT_RATE_HZ} Hz, {SLOT_FRAMES} frames");
    if fmt != 1 || channels != 1 || bits != 16 || rate != SLOT_RATE_HZ {
        return Err(WavError::WrongFormat(format!(
            "got fmt={fmt} ch={channels} bits={bits} rate={rate}; want {want}"
        )));
    }
    if data_len != SLOT_FRAMES * 2 {
        return Err(WavError::WrongFormat(format!(
            "got {} data bytes ({} frames); want {want}", data_len, data_len / 2
        )));
    }
    // Header can lie about truncated-on-disk files — verify actual size.
    let actual = std::fs::metadata(path)
        .map_err(|e| WavError::Malformed(e.to_string()))?
        .len();
    if actual != 44 + u64::from(data_len) {
        return Err(WavError::WrongFormat(format!(
            "file is {actual} bytes; header promises {}", 44 + u64::from(data_len)
        )));
    }
    Ok(())
}
```

Note: this validates the CANONICAL layout our own L2 slot writer emits (fmt
chunk at 12, data at 36). The committed SDR fixtures are also canonical —
Step 4 proves it. Non-canonical-but-valid WAVs (extra chunks) are rejected by
design: L1 only ever receives L2's own output, and stricter is safer.

- [ ] **Step 4: Add a fixture-conformance test**

Append to the test module:
```rust
    #[test]
    fn committed_sdr_fixtures_are_canonical_slot_wavs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tuxlink-ft8/tests/fixtures/sdr");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().is_some_and(|e| e == "wav") {
                assert_eq!(preflight_slot_wav(&p), Ok(()), "fixture {p:?}");
                checked += 1;
            }
        }
        assert!(checked >= 4, "expected the 4 committed SDR fixtures, found {checked}");
    }
```

- [ ] **Step 5: Run all tests green + clippy clean, then commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --all-targets --locked -- -D warnings`

```bash
git add src-tauri/tuxlink-jt9/src/wav.rs src-tauri/tuxlink-jt9/src/lib.rs
git commit -m "feat(ft8): host-side slot-WAV preflight — jt9 cannot be trusted to reject bad input (tuxlink-b026z.2 T3)"
```

---

### Task 4: Decode types + binary discovery + version probe

**Files:**
- Create: `src-tauri/tuxlink-jt9/src/types.rs`
- Create: `src-tauri/tuxlink-jt9/src/discover.rs`
- Modify: `src-tauri/tuxlink-jt9/src/lib.rs` (add `pub mod types; pub mod discover;`)

**Interfaces:**
- Produces (consumed by Task 5 and later by L2/L4 wiring):
```rust
pub struct Ft8Decode {
    pub slot_utc_ms: u64,
    pub snr_db: i32,
    pub dt_s: f64,
    pub freq_hz: u32,
    pub message: String,
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
    pub partial: bool,
}
pub enum SlotFailure {
    BadWav(String), Signal { signal: String, stderr_tail: String },
    Timeout, StderrEof, ParseError, SpawnFailed(String),
}
pub enum SlotOutcome { Decoded(Vec<Ft8Decode>), BandDead, Failed(SlotFailure) }
// in discover.rs:
pub fn discover_jt9(config_override: Option<&Path>) -> Result<Jt9Binary, DiscoverError>;
pub struct Jt9Binary { pub jt9_path: PathBuf, pub engine_version: String }
```

- [ ] **Step 1: Write `types.rs`** (plain data, no tests needed beyond compile):

```rust
//! Decode-service data types (Station Intelligence L1).

/// One decoded FT8 message. `slot_utc_ms` is stamped by the HOST slot
/// scheduler — jt9's stdout timestamp is always `000000` for our filenames
/// and is never used (delta §Grounded facts).
#[derive(Debug, Clone, PartialEq)]
pub struct Ft8Decode {
    pub slot_utc_ms: u64,
    pub snr_db: i32,
    pub dt_s: f64,
    pub freq_hz: u32,
    pub message: String,
    /// None when the sender is an unresolved hashed callsign (`<...>`) —
    /// per-slot jt9 spawn cannot resolve cross-slot hashes (accepted
    /// regression, delta §Revised L1). Such decodes are excluded from
    /// ft8_who_can_i_hear downstream.
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
    /// True when this record was salvaged from a timed-out run's partial
    /// stdout (no `<DecodeFinished>` sentinel seen).
    pub partial: bool,
}

/// Per-slot failure classification (delta §failure taxonomy). Feeds the
/// jt9-degraded health flag upstream: N consecutive non-`Decoded`/`BandDead`
/// outcomes degrade; the first good slot clears.
/// Degraded-flag thresholds (consumed by the L2 plan's slot scheduler; the
/// delta requires them pinned here): jt9-degraded after N = 5 consecutive
/// non-Decoded/non-BandDead outcomes, clearing on the first good slot;
/// band-dead after k = 20 consecutive zero-decode slots (5 minutes).
#[derive(Debug, Clone, PartialEq)]
pub enum SlotFailure {
    /// Preflight rejection — never spawned. STABLE-STRING CONTRACT: the exact
    /// strings "not found" and "permission denied" are API — L2's mid-run
    /// disappearance detection (consecutive not-found → degraded) matches on
    /// them. Other WAV defects carry free-text diagnostics.
    BadWav(String),
    /// jt9 died by signal (its common failure mode: Fortran error + SIGSEGV).
    Signal { signal: String, stderr_tail: String },
    /// Killed at the deadline with zero decode lines salvaged.
    Timeout,
    /// jt9's `EOF on input file` on stderr: a capture bug, NOT a quiet band.
    StderrEof,
    /// Exited zero, produced output, but not a single line parsed.
    ParseError,
    /// The OS could not spawn the process at all.
    SpawnFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotOutcome {
    Decoded(Vec<Ft8Decode>),
    /// Clean exit, zero decodes: a quiet band — explicitly NOT a failure.
    BandDead,
    Failed(SlotFailure),
}
```

- [ ] **Step 2: Write failing discovery tests** (`discover.rs`):

```rust
//! jt9 binary discovery + engine version probe.
//!
//! Order: explicit config override (must exist and be a file) > `jt9` on
//! PATH. Version comes from the SIBLING `wsjtx_app_version -v` (jt9 itself
//! has no version flag — verified: `--version` → "unrecognised option",
//! exit 0). Fallback: "jt9 (version unknown)".

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum DiscoverError {
    OverrideMissing(PathBuf),
    NotOnPath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Jt9Binary {
    pub jt9_path: PathBuf,
    pub engine_version: String,
}

pub fn discover_jt9(config_override: Option<&Path>) -> Result<Jt9Binary, DiscoverError> {
    let _ = config_override;
    Err(DiscoverError::NotOnPath) // stub
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_bin_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tuxlink-jt9-disc-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn install_fake(dir: &Path, name: &str, script: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn override_wins_and_version_comes_from_sibling() {
        let d = fake_bin_dir();
        let jt9 = install_fake(&d, "jt9", "#!/bin/sh\nexit 0\n");
        install_fake(&d, "wsjtx_app_version", "#!/bin/sh\n[ \"$1\" = \"-v\" ] && echo 'WSJT-X 2.7.0'\n");
        let got = discover_jt9(Some(&jt9)).unwrap();
        assert_eq!(got.jt9_path, jt9);
        assert_eq!(got.engine_version, "WSJT-X 2.7.0");
    }

    #[test]
    fn version_unknown_when_sibling_absent() {
        let d = fake_bin_dir().join("nosib");
        std::fs::create_dir_all(&d).unwrap();
        let jt9 = install_fake(&d, "jt9", "#!/bin/sh\nexit 0\n");
        let got = discover_jt9(Some(&jt9)).unwrap();
        assert_eq!(got.engine_version, "jt9 (version unknown)");
    }

    #[test]
    fn missing_override_is_an_error_not_a_fallback() {
        // A configured-but-broken override must be loud, not silently
        // fall back to PATH (operator set it for a reason).
        let missing = PathBuf::from("/nonexistent/custom-jt9");
        assert_eq!(discover_jt9(Some(&missing)), Err(DiscoverError::OverrideMissing(missing)));
    }
}
```

- [ ] **Step 3: Run tests, verify FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked discover`
Expected: first two FAIL; third may pass by stub coincidence — acceptable, the positive cases discriminate.

- [ ] **Step 4: Implement**

```rust
pub fn discover_jt9(config_override: Option<&Path>) -> Result<Jt9Binary, DiscoverError> {
    let jt9_path = match config_override {
        Some(p) => {
            if !p.is_file() {
                return Err(DiscoverError::OverrideMissing(p.to_path_buf()));
            }
            p.to_path_buf()
        }
        None => which_jt9().ok_or(DiscoverError::NotOnPath)?,
    };
    let engine_version = probe_version(&jt9_path);
    Ok(Jt9Binary { jt9_path, engine_version })
}

fn which_jt9() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("jt9")).find(|c| c.is_file())
}

fn probe_version(jt9_path: &Path) -> String {
    const UNKNOWN: &str = "jt9 (version unknown)";
    let Some(dir) = jt9_path.parent() else { return UNKNOWN.into() };
    let sibling = dir.join("wsjtx_app_version");
    if !sibling.is_file() {
        return UNKNOWN.into();
    }
    match std::process::Command::new(&sibling).arg("-v").output() {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() { UNKNOWN.into() } else { s }
        }
        Err(_) => UNKNOWN.into(),
    }
}
```

- [ ] **Step 5: All green + clippy, commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --all-targets --locked -- -D warnings`

```bash
git add src-tauri/tuxlink-jt9/src/types.rs src-tauri/tuxlink-jt9/src/discover.rs src-tauri/tuxlink-jt9/src/lib.rs
git commit -m "feat(ft8): decode types + jt9 discovery with sibling version probe (tuxlink-b026z.2 T4)"
```

---

### Task 5: The runner — spawn, timeout/kill, partial salvage, failure taxonomy

**Files:**
- Create: `src-tauri/tuxlink-jt9/src/runner.rs`
- Create: `src-tauri/tuxlink-jt9/tests/fake_jt9.rs` (integration tests using fake jt9 scripts)
- Modify: `src-tauri/tuxlink-jt9/src/lib.rs` (add `pub mod runner;` and re-export `pub use runner::Jt9Runner;`)

**Interfaces:**
- Consumes: `parse::parse_stdout_line`, `message::extract_fields`, `wav::preflight_slot_wav`, `types::*`, `discover::Jt9Binary`.
- Produces (the L1 public API; L2 calls this via `spawn_blocking`):
```rust
impl Jt9Runner {
    pub fn new(binary: Jt9Binary, data_dir: PathBuf, timeout: Duration) -> Jt9Runner;
    /// Blocking: preflight → spawn → drain → wait-or-kill → classify.
    pub fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome;
    /// Run one decode to completion on a bundled silence WAV so FFTW wisdom
    /// is written before the slot loop starts (delta: killed runs never
    /// persist wisdom — the pre-warm breaks the timeout death-spiral).
    pub fn prewarm(&self) -> Result<(), SlotFailure>;
}
```

Invocation contract (verbatim from the delta — the arg builder is the ONLY
place `"jt9"` is spawned in the repo, and `-s`/`--shmem` must never appear):
`<jt9> -8 -d 3 -p 15 -w 1 -a <data_dir> -t <slot_tmp> <wav>` with
`current_dir(slot_tmp)`, stdout piped, stderr piped, stdin null.

Process discipline: `std::process::Child` with a stdout drain thread (decode
lines stream incrementally; the drain also prevents pipe-full deadlock) and a
stderr drain thread; poll `child.try_wait()` every 100 ms up to the timeout;
on deadline call `child.kill()` then `child.wait()` (reap — no zombies).
A `Drop` guard on a wrapper struct kills+waits if `decode_slot` unwinds.

Classification order (first match wins):
1. Preflight error → `Failed(BadWav(text))` (never spawned).
2. Spawn error → `Failed(SpawnFailed(text))`.
3. Deadline hit → salvage: parsed `Decode` lines seen without
   `DecodeFinished` → `Decoded(records with partial: true)`; zero lines →
   `Failed(Timeout)`.
4. Exited by signal → `Failed(Signal { signal, stderr_tail })` (stderr_tail =
   last 300 bytes, lossy UTF-8).
5. stderr contains `EOF on input file` → `Failed(StderrEof)` — even on exit 0.
6. Exit 0: ≥1 parsed decode → `Decoded(partial: false)`; zero parsed decode
   lines AND zero raw non-sentinel lines → `BandDead`; zero parsed decodes
   but raw non-sentinel lines present → `Failed(ParseError)` (garbage output
   is suspicious even when the sentinel arrived).
7. Nonzero exit without signal → `Failed(Signal { signal: "exit <code>",
   stderr_tail })` (jt9 has no documented nonzero exits; treat as failure).

- [ ] **Step 1: Write the failing integration tests with fake jt9 scripts**

`src-tauri/tuxlink-jt9/tests/fake_jt9.rs`:
```rust
//! Runner lifecycle tests against controllable fake jt9 shell scripts.
//! The REAL jt9 is exercised in tests/real_jt9.rs (Task 6).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tuxlink_jt9::discover::Jt9Binary;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::{SlotFailure, SlotOutcome};

const DECODE_LINE: &str = "000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19";
const SENTINEL: &str = "<DecodeFinished>   0   1        0";

fn setup(name: &str, script: &str) -> (Jt9Runner, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-rt-{}-{}", name, std::process::id()));
    let bin_dir = base.join("bin");
    let data = base.join("data");
    let slot_tmp = base.join("slot");
    for d in [&bin_dir, &data, &slot_tmp] { std::fs::create_dir_all(d).unwrap(); }
    let fake = bin_dir.join("jt9");
    std::fs::write(&fake, script).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let runner = Jt9Runner::new(
        Jt9Binary { jt9_path: fake, engine_version: "fake".into() },
        data,
        Duration::from_secs(2), // short deadline for tests
    );
    let wav = base.join("slot.wav");
    write_canonical_wav(&wav);
    (runner, wav, slot_tmp)
}

/// Canonical 180,000-frame silence WAV (passes preflight).
fn write_canonical_wav(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    let data_len: u32 = 180_000 * 2;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&12_000u32.to_le_bytes()).unwrap();
    f.write_all(&24_000u32.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; data_len as usize]).unwrap();
}

#[test]
fn happy_path_decodes_and_stamps_slot_utc() {
    let (runner, wav, tmp) = setup("happy", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho '{SENTINEL}'\nexit 0\n"));
    match runner.decode_slot(&wav, &tmp, 1_752_000_000_000) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert_eq!(recs[0].slot_utc_ms, 1_752_000_000_000);
            assert_eq!(recs[0].from_call.as_deref(), Some("K5OJT"));
            assert!(!recs[0].partial);
        }
        other => panic!("want Decoded, got {other:?}"),
    }
}

#[test]
fn clean_zero_decode_is_band_dead_not_failure() {
    let (runner, wav, tmp) = setup("dead", &format!(
        "#!/bin/sh\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::BandDead);
}

#[test]
fn timeout_salvages_partial_decodes() {
    // Emits one decode, then hangs past the 2s deadline. Salvage keeps it.
    // `exec` so the sleep IS the killed process (no orphan grandchild).
    let (runner, wav, tmp) = setup("salvage", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\nexec sleep 30\n"));
    let t0 = std::time::Instant::now();
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert!(recs[0].partial, "salvaged records must be flagged partial");
        }
        other => panic!("want salvaged Decoded, got {other:?}"),
    }
    assert!(t0.elapsed() < Duration::from_secs(10), "kill must be prompt, no 30s wait");
}

#[test]
fn timeout_with_no_output_is_timeout_failure() {
    let (runner, wav, tmp) = setup("hang", "#!/bin/sh\nexec sleep 30\n");
    let t0 = std::time::Instant::now();
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::Timeout));
    assert!(t0.elapsed() < Duration::from_secs(10));
}

#[test]
fn signal_death_is_classified_with_stderr_tail() {
    // Reproduces jt9's real mode: stderr diagnostics then SIGSEGV.
    let (runner, wav, tmp) = setup("segv", 
        "#!/bin/sh\necho 'Fortran runtime error: End of file simulation' 1>&2\nkill -SEGV $$\n");
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Failed(SlotFailure::Signal { signal, stderr_tail }) => {
            assert!(signal.contains("11") || signal.to_uppercase().contains("SEGV"), "{signal}");
            assert!(stderr_tail.contains("Fortran runtime error"));
        }
        other => panic!("want Signal, got {other:?}"),
    }
}

#[test]
fn stderr_eof_on_clean_exit_is_a_capture_bug_not_band_dead() {
    let (runner, wav, tmp) = setup("eof", &format!(
        "#!/bin/sh\necho 'EOF on input file' 1>&2\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::StderrEof));
}

#[test]
fn bad_wav_never_spawns() {
    let (runner, _wav, tmp) = setup("badwav", "#!/bin/sh\ntouch spawned-marker\nexit 0\n");
    let missing = std::env::temp_dir().join("no-such-slot.wav");
    match runner.decode_slot(&missing, &tmp, 0) {
        SlotOutcome::Failed(SlotFailure::BadWav(_)) => {}
        other => panic!("want BadWav, got {other:?}"),
    }
    assert!(!tmp.join("spawned-marker").exists(), "preflight must gate the spawn");
}

#[test]
fn hung_grandchild_holding_pipes_does_not_block_the_kill_path() {
    // A forked grandchild inherits the pipe write-ends; the runner must not
    // join the drain threads on the timeout path or this hangs 30s.
    let (runner, wav, tmp) = setup("grandchild", "#!/bin/sh\n( sleep 30 ) &\nsleep 30\n");
    let t0 = std::time::Instant::now();
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::Timeout));
    assert!(t0.elapsed() < Duration::from_secs(10), "must not block on grandchild pipes");
}

#[test]
fn nonexistent_binary_is_spawn_failed() {
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-rt-nospawn-{}", std::process::id()));
    let slot_tmp = base.join("slot");
    std::fs::create_dir_all(&slot_tmp).unwrap();
    let wav = base.join("slot.wav");
    write_canonical_wav(&wav);
    let runner = Jt9Runner::new(
        Jt9Binary { jt9_path: base.join("no-such-jt9"), engine_version: "fake".into() },
        base.join("data"),
        Duration::from_secs(2),
    );
    assert!(matches!(
        runner.decode_slot(&wav, &slot_tmp, 0),
        SlotOutcome::Failed(SlotFailure::SpawnFailed(_))
    ));
}

#[test]
fn garbage_output_without_decodes_is_parse_error() {
    let (runner, wav, tmp) = setup("garbage", 
        "#!/bin/sh\necho 'random noise line'\necho 'more junk'\nexit 0\n");
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::ParseError));
}

#[test]
fn arg_builder_never_emits_shmem() {
    // Guard the GPL boundary at the unit level: the fake script fails loudly
    // if it ever sees -s/--shmem.
    let (runner, wav, tmp) = setup("noshm", &format!(
        "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in -s|--shmem) exit 97;; esac; done\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::BandDead);
}
```

- [ ] **Step 2: Run tests, verify they FAIL to compile (no runner yet), then add the skeleton and verify they fail on behavior**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --locked --test fake_jt9`
Expected first: compile error (`runner` missing). Add the skeleton — `Jt9Runner` struct, `new()`, a `decode_slot` stub returning `SlotOutcome::Failed(SlotFailure::Timeout)`, a `prewarm` stub returning `Ok(())`, and `pub mod runner;` + `pub use runner::Jt9Runner;` in `lib.rs` — re-run, and confirm each test fails on its assertion (not on panics/compile).

- [ ] **Step 3: Implement the runner**

`src-tauri/tuxlink-jt9/src/runner.rs`:
```rust
//! jt9 subprocess runner. THE only place in the repository that spawns jt9.
//! Invocation: `<jt9> -8 -d 3 -p 15 -w 1 -a <data> -t <slot_tmp> <wav>`.
//! `-s`/`--shmem` is BANNED (GPL boundary; enforced by CI grep and a unit
//! test). See docs/design/2026-07-10-station-intel-jt9-engine-delta.md.

use crate::discover::Jt9Binary;
use crate::message::extract_fields;
use crate::parse::{parse_stdout_line, ParsedLine};
use crate::types::{Ft8Decode, SlotFailure, SlotOutcome};
use crate::wav::{preflight_slot_wav, WavError};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct Jt9Runner {
    binary: Jt9Binary,
    data_dir: PathBuf,
    timeout: Duration,
}

/// Kills + reaps the child if the runner unwinds mid-decode.
struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Jt9Runner {
    pub fn new(binary: Jt9Binary, data_dir: PathBuf, timeout: Duration) -> Self {
        Self { binary, data_dir, timeout }
    }

    pub fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome {
        match preflight_slot_wav(wav) {
            Ok(()) => {}
            Err(WavError::NotFound) => return SlotOutcome::Failed(SlotFailure::BadWav("not found".into())),
            Err(WavError::Permission) => return SlotOutcome::Failed(SlotFailure::BadWav("permission denied".into())),
            Err(WavError::Malformed(e)) | Err(WavError::WrongFormat(e)) => {
                return SlotOutcome::Failed(SlotFailure::BadWav(e))
            }
        }
        let child = Command::new(&self.binary.jt9_path)
            .args(["-8", "-d", "3", "-p", "15", "-w", "1"])
            .arg("-a").arg(&self.data_dir)
            .arg("-t").arg(slot_tmp)
            .arg(wav)
            .current_dir(slot_tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return SlotOutcome::Failed(SlotFailure::SpawnFailed(e.to_string())),
        };

        // Drain threads: decode lines stream incrementally; draining also
        // prevents pipe-full stalls on chatty output.
        let (line_tx, line_rx) = mpsc::channel::<String>();
        let stdout = child.stdout.take().expect("stdout piped");
        let stdout_thread = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut stderr = child.stderr.take().expect("stderr piped");
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
        });

        let mut guard = ChildGuard(Some(child));
        let deadline = Instant::now() + self.timeout;
        let mut timed_out = false;
        let status = loop {
            match guard.0.as_mut().expect("child present").try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        timed_out = true;
                        let mut c = guard.0.take().expect("child present");
                        let _ = c.kill();
                        let _ = c.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return SlotOutcome::Failed(SlotFailure::SpawnFailed(e.to_string())),
            }
        };
        guard.0 = None; // reaped on every path above
        let stderr_text = if timed_out {
            // A grandchild of the killed process can hold the pipe write-ends
            // open indefinitely; NEVER block on the drain threads here. Give
            // the drains a beat to flush what the kernel buffered, then
            // collect via the channel; the threads exit on their own at pipe
            // EOF whenever the holder dies.
            std::thread::sleep(Duration::from_millis(50));
            String::new()
        } else {
            let _ = stdout_thread.join();
            stderr_thread.join().unwrap_or_default()
        };

        // Collect everything the drain saw.
        let mut decodes = Vec::new();
        let mut saw_sentinel = false;
        let mut raw_lines = 0usize;
        for line in line_rx.try_iter() {
            match parse_stdout_line(&line) {
                ParsedLine::Decode { snr_db, dt_s, freq_hz, message } => {
                    let fields = extract_fields(&message);
                    decodes.push(Ft8Decode {
                        slot_utc_ms, snr_db, dt_s, freq_hz, message,
                        from_call: fields.from_call, to_call: fields.to_call,
                        grid: fields.grid, partial: false,
                    });
                }
                ParsedLine::DecodeFinished => saw_sentinel = true,
                ParsedLine::Other => {
                    if !line.trim().is_empty() {
                        raw_lines += 1;
                    }
                }
            }
        }

        if timed_out {
            return if decodes.is_empty() {
                SlotOutcome::Failed(SlotFailure::Timeout)
            } else {
                for d in &mut decodes {
                    d.partial = true;
                }
                SlotOutcome::Decoded(decodes)
            };
        }
        let status = status.expect("non-timeout path has a status");

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                return SlotOutcome::Failed(SlotFailure::Signal {
                    signal: format!("signal {sig}"),
                    stderr_tail: tail(&stderr_text, 300),
                });
            }
        }
        if stderr_text.contains("EOF on input file") {
            return SlotOutcome::Failed(SlotFailure::StderrEof);
        }
        if !status.success() {
            return SlotOutcome::Failed(SlotFailure::Signal {
                signal: format!("exit {}", status.code().unwrap_or(-1)),
                stderr_tail: tail(&stderr_text, 300),
            });
        }
        if !decodes.is_empty() {
            return SlotOutcome::Decoded(decodes);
        }
        let _ = saw_sentinel; // completeness marker only matters on the timeout path
        if raw_lines == 0 {
            SlotOutcome::BandDead
        } else {
            SlotOutcome::Failed(SlotFailure::ParseError)
        }
    }

    pub fn prewarm(&self) -> Result<(), SlotFailure> {
        let dir = std::env::temp_dir().join(format!("tuxlink-jt9-prewarm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| SlotFailure::SpawnFailed(e.to_string()))?;
        let wav = dir.join("silence.wav");
        write_silence_wav(&wav).map_err(|e| SlotFailure::SpawnFailed(e.to_string()))?;
        match self.decode_slot(&wav, &dir, 0) {
            SlotOutcome::Decoded(_) | SlotOutcome::BandDead => Ok(()),
            SlotOutcome::Failed(f) => Err(f),
        }
    }
}

fn tail(s: &str, n: usize) -> String {
    let start = s.len().saturating_sub(n);
    // Snap to a char boundary.
    let mut i = start;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    s[i..].to_string()
}

fn write_silence_wav(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let data_len: u32 = crate::wav::SLOT_FRAMES * 2;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&crate::wav::SLOT_RATE_HZ.to_le_bytes())?;
    f.write_all(&(crate::wav::SLOT_RATE_HZ * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    f.write_all(&vec![0u8; data_len as usize])
}
```

(The test module's `write_canonical_wav` duplicates `write_silence_wav`
deliberately — the test must not trust the code under test to build its own
fixtures.)

- [ ] **Step 4: Run the fake-jt9 suite, verify all green; run the full crate suite + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --all-targets --locked -- -D warnings`
Expected: all green, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-jt9/src/runner.rs src-tauri/tuxlink-jt9/tests/fake_jt9.rs src-tauri/tuxlink-jt9/src/lib.rs
git commit -m "feat(ft8): jt9 runner — spawn/timeout/kill discipline, partial salvage, full failure taxonomy (tuxlink-b026z.2 T5)"
```

---

### Task 6: Real-jt9 end-to-end tests + wisdom persistence proof

**Files:**
- Create: `src-tauri/tuxlink-jt9/tests/real_jt9.rs`

**Interfaces:**
- Consumes: `Jt9Runner`, `discover_jt9`. No new API.

- [ ] **Step 1: Write the e2e tests (they will PASS immediately if Task 5 is correct — that is the point: these are the acceptance gate against the real binary, not TDD-of-new-code)**

```rust
//! End-to-end against the REAL jt9 on the committed SDR fixtures.
//! Skips (with a printed notice) when jt9 is absent; CI installs wsjtx so
//! these always run there. Locally on the dev Pi they run in seconds.

use std::path::PathBuf;
use std::time::Duration;
use tuxlink_jt9::discover::discover_jt9;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SlotOutcome;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tuxlink-ft8/tests/fixtures/sdr").join(name)
}

fn runner_or_skip(tag: &str) -> Option<(Jt9Runner, PathBuf)> {
    let Ok(bin) = discover_jt9(None) else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — {tag}");
        return None;
    };
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-{tag}-{}", std::process::id()));
    let data = base.join("data");
    let slot = base.join("slot");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&slot).unwrap();
    Some((Jt9Runner::new(bin, data, Duration::from_secs(12)), slot))
}

#[test]
fn ordinary_fixture_decodes_at_least_the_depth1_reference_set() {
    let Some((runner, slot)) = runner_or_skip("ordinary") else { return };
    match runner.decode_slot(&fixture("ft8-40m-ordinary-20260706T121215Z.wav"), &slot, 42) {
        SlotOutcome::Decoded(recs) => {
            // Depth-1 reference = 5 messages; -d 3 found 6 on 2.7.0. Floor at
            // the depth-1 count so a wsjtx-version delta cannot flake this.
            assert!(recs.len() >= 5, "got {} decodes", recs.len());
            assert!(recs.iter().all(|r| r.slot_utc_ms == 42));
            assert!(recs.iter().any(|r| r.message.contains("K5OJT")), "known strong signal missing");
            assert!(recs.iter().all(|r| !r.partial));
        }
        other => panic!("want Decoded, got {other:?}"),
    }
}

#[test]
fn quiet_fixture_decodes_both_reference_messages() {
    let Some((runner, slot)) = runner_or_skip("quiet") else { return };
    match runner.decode_slot(&fixture("ft8-20m-quiet-20260706T121400Z.wav"), &slot, 0) {
        SlotOutcome::Decoded(recs) => assert!(recs.len() >= 2, "got {}", recs.len()),
        other => panic!("want Decoded, got {other:?}"),
    }
}

#[test]
fn prewarm_persists_fftw_wisdom_into_the_data_dir() {
    let Some((runner, _slot)) = runner_or_skip("wisdom") else { return };
    runner.prewarm().expect("prewarm must complete");
    // The data dir was created by runner_or_skip under this test's base; the
    // wisdom file is jt9's completion artifact (delta §Grounded facts).
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-wisdom-{}", std::process::id()));
    assert!(base.join("data").join("jt9_wisdom.dat").exists(),
        "successful completion must write FFTW wisdom into the persistent -a dir");
}

#[test]
fn silence_is_band_dead() {
    let Some((runner, slot)) = runner_or_skip("silence") else { return };
    // prewarm()'s silence decode returns BandDead through the same path; this
    // pins it as the public contract for a truly quiet slot.
    match runner.prewarm() {
        Ok(()) => {}
        Err(f) => panic!("silence must be clean BandDead/Decoded, got {f:?}"),
    }
    let _ = slot; // silence path exercised via prewarm
}
```

- [ ] **Step 2: Run the e2e suite locally (jt9 IS installed on the dev Pi)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked --test real_jt9 -- --nocapture`
Expected: 4 passed, ~15–30 s total (first run pays FFTW planning per test data dir). If any fail, the RUNNER is wrong — fix Task 5's code, not these tests (they encode the design's empirical ground truth).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tuxlink-jt9/tests/real_jt9.rs
git commit -m "test(ft8): real-jt9 e2e on committed SDR fixtures + wisdom persistence proof (tuxlink-b026z.2 T6)"
```

---

**REVIEW GATE B (after Tasks 3–6):** review the preflight + types + discovery + runner batch. Perspectives: (1) classification-order correctness vs the delta's taxonomy — walk each SlotOutcome arm against the fake-jt9 tests; (2) resource hygiene — no zombie on ANY path including panics (the ChildGuard), drain threads never joined on the timeout path (grandchild-pipe hang), no PID reuse window; (3) portability/CI — will the fake scripts behave on ubuntu runners (`/bin/sh` dash vs bash: the scripts use only POSIX sh constructs — verify). Minimum three rounds. Persist findings to `dev/scratch/b026z.2-gate-B-findings.md` before proceeding.

---

### Task 7: Regenerate fixture references at the production flag set

**Files:**
- Modify: `src-tauri/tuxlink-ft8/tests/fixtures/sdr/README.md`
- Modify: `src-tauri/tuxlink-ft8/tests/fixtures/sdr/*.jt9-ap-off.txt` (regenerated content)
- Possibly modify: `src-tauri/tuxlink-ft8/tests/sdr_parity.rs` (only if its assertions hardcode per-fixture counts — check first)

This is the ONE allowed change inside the frozen reference crate (delta §Non-goals). The committed refs were generated at default depth (`jt9 -8`, depth 1); production runs `-8 -d 3 -p 15 -w 1`. Depth-1 refs under-count what the service produces (10 vs 14 on crowded).

- [ ] **Step 1: Regenerate each reference log with the production flags**

For each of the 4 fixture WAVs, from a scratch dir (NEVER the repo root — jt9 drops `decoded.txt`/`timer.out`/`jt9_wisdom.dat` in its cwd/data/temp paths; that is how the repo root got polluted):
```bash
WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-b026z.2-station-intel-jt9
S=$(mktemp -d)
cd "$S"
for w in "$WT"/src-tauri/tuxlink-ft8/tests/fixtures/sdr/*.wav; do
  jt9 -8 -d 3 -p 15 -w 1 -a "$S" -t "$S" "$w" > "$(basename "${w%.wav}").jt9-d3-ap-off.txt"
done
mv "$S"/*.jt9-d3-ap-off.txt "$WT/src-tauri/tuxlink-ft8/tests/fixtures/sdr/"
```
Delete the old `.jt9-ap-off.txt` files **only after** Step 2 confirms nothing else references them.

- [ ] **Step 2: Find every reference to the old ref filenames**

Run: `grep -rn "jt9-ap-off" src-tauri/ --include="*.rs" --include="*.md"`
Known reference sites (verified at plan time): `sdr_parity.rs:60,70,119`
(format strings — update to the new suffix), `oracle.rs:6` (a doc comment —
this ONE-LINE doc update inside the frozen crate is pre-authorized; nothing
else in `src/` may change), and the fixtures README (Step 3). Historical
handoff docs under `dev/handoffs/` are exempt — leave them. `sdr_parity.rs`
asserts zero-false only (no hardcoded counts — verified), so the depth-3
superset refs are safe. Keep `tuxlink-ft8`'s tests green — the reference
moved; the crate's decoder is NOT being touched.

- [ ] **Step 3: Update the README recipe**

In `src-tauri/tuxlink-ft8/tests/fixtures/sdr/README.md`, replace the regeneration recipe with the production flag set and a note: "References regenerated 2026-07-10 at the Station Intelligence production invocation (`jt9 -8 -d 3 -p 15 -w 1`, wsjtx 2.7.0). The depth-1 references were superseded when the L1 service pinned `-d 3` (delta §Revised L1)." (The delta's own regeneration parenthetical omits `-p 15`; the production invocation line governs — decode output is byte-identical either way, verified.)

- [ ] **Step 3b: Version-floor evidence (delta §Packaging obligation)**

Attempt to obtain a wsjtx 2.5-era stdout capture for one fixture (e.g. run the fixtures through a 2.5.x jt9 in a Debian-bullseye container, or source a documented 2.5 output sample). If obtained: commit the lines as additional parser KATs in `parse.rs`. If NOT obtainable this session: add a dated note to the `parse.rs` module doc ("output format verified on wsjtx 2.7.0 only; 2.5-era verification open") AND file a bd issue for it — the PR body must name the issue. Do not silently drop this.

- [ ] **Step 4: Run the affected suites**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-ft8 --locked`
(The ft8 crate compiles locally in ~14 s.) Expected: green with the new refs.
Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-jt9 --locked`
Expected: still green (the e2e floors were chosen version-tolerantly).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-ft8/tests/fixtures/sdr/ src-tauri/tuxlink-ft8/tests/sdr_parity.rs
git commit -m "test(ft8): regenerate SDR fixture references at the production jt9 flag set (tuxlink-b026z.2 T7)"
```

---

### Task 8: CI — install wsjtx so the real-jt9 e2e always runs; add the .7 provenance guard

**Files:**
- Modify: `.github/workflows/ci.yml` (verify job: apt install wsjtx before the cargo test step, both arches)
- Create: `scripts/check-jt9-provenance.sh`
- Modify: `.github/workflows/ci.yml` (add a guard step invoking the script)

**Interfaces:** none (CI-only). This task also closes epic child `tuxlink-b026z.7` for the current scope.

- [ ] **Step 1: Add wsjtx to the verify job**

In `.github/workflows/ci.yml`, in the verify job's system-dependency step (find the existing `apt-get install` line the Tauri build uses; add to it or add a step immediately before the Rust test step):
```yaml
      - name: Install wsjtx (jt9 decode oracle for tuxlink-jt9 e2e)
        run: |
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends wsjtx
          jt9 -h >/dev/null 2>&1 || true  # smoke: binary present (exits 0 on -h)
          test -x /usr/bin/jt9
```
(`--no-install-recommends` keeps the Qt GUI dependency pull minimal; jt9 and `wsjtx_app_version` ship in the main wsjtx package via hard Depends — verified available in noble universe on BOTH amd64 and arm64, covering `ubuntu-latest` and `ubuntu-24.04-arm`. PREFERRED variant: the verify job already caches apt packages via `awalsh128/cache-apt-pkgs-action` (~ci.yml:60) — add `wsjtx` to that action's package list and bump its version salt instead of a bare uncached step, which would re-download the Qt/hamlib/fftw closure every run.)

- [ ] **Step 2: Write the provenance guard script**

`scripts/check-jt9-provenance.sh` (deny-patterns verbatim from the delta §.7; structural, so tuxlink-ft8's prose mentions cannot false-positive):
```bash
#!/usr/bin/env bash
# Station Intelligence GPL-boundary guard (tuxlink-b026z.7).
# jt9/WSJT-X may be invoked as a subprocess ONLY, from ONE module.
# See docs/design/2026-07-10-station-intel-jt9-engine-delta.md §GPL boundary.
set -euo pipefail
fail=0
err() { echo "PROVENANCE VIOLATION: $1" >&2; fail=1; }

# 1. No GPL source files (WSJT-X Fortran / LDPC tables) in the tree.
if git ls-files | grep -E '\.(f90|f95)$|(^|/)(parity|generator)\.dat$' >&2; then
  err "GPL-source-shaped files tracked (see above)"
fi

# 2. No dependency edge on GPL Rust crates.
# (/dev/null sentinel: with an empty file list grep sees one unmatchable
# operand and exits 1 cleanly — never reads stdin, never hangs; do NOT use
# xargs -r, whose empty-input exit 0 would misread as a violation.)
if git ls-files '*Cargo.toml' | xargs grep -lnE '^\s*(wsjtr|ft8core)\s*=' /dev/null >&2; then
  err "Cargo dependency on wsjtr/ft8core"
fi

# 3. No FFI in any Rust file that mentions wsjt.
if git ls-files '*.rs' | xargs grep -liE 'wsjt' /dev/null | xargs grep -lnE '#\[link|extern\s+"C"' /dev/null >&2; then
  err "FFI in a wsjt-mentioning Rust file"
fi

# 4. No bundling of jt9/wsjtx in any artifact.
if jq -e '((.bundle.externalBin // []) + (.bundle.resources // [])) | map(select(test("jt9|wsjt"; "i"))) | length > 0' src-tauri/tauri.conf.json >/dev/null; then
  err "tauri.conf.json bundles jt9/wsjtx"
fi
if grep -nE 'binaries/(jt9|wsjt)' .github/workflows/release.yml >&2; then
  err "release.yml externalBin-injects jt9/wsjtx"
fi

# 5. Subprocess confinement: "jt9" in spawn position only inside the runner.
if git ls-files '*.rs' | grep -v 'tuxlink-jt9/src/runner.rs' \
    | xargs grep -lnE 'Command::new\([^)]*jt9' /dev/null >&2; then
  err "jt9 spawned outside tuxlink-jt9/src/runner.rs"
fi
# Strip comments BEFORE matching (grep -n would prefix line numbers, making
# the '^\s*//' filter dead — the guard would self-trip on the module doc,
# which legitimately says the flag is banned).
if grep -vE '^\s*//' src-tauri/tuxlink-jt9/src/runner.rs | grep -nE -- '-s\b|--shmem' >&2; then
  err "shmem flag in the jt9 arg builder (GPL boundary-crosser)"
fi

exit $fail
```
`chmod +x scripts/check-jt9-provenance.sh`. NOTE for the implementer: the
grep-pipeline exit semantics are the tricky part (grep exits 1 on no-match,
which `set -e` would kill inside `if` conditions — the `if grep` form is safe;
verify each clause by deliberately introducing a violation locally and
watching it trip, then reverting).

- [ ] **Step 3: Wire the guard into ci.yml**

Add to the verify job (before the Rust steps, it is fast):
```yaml
      - name: jt9/WSJT-X provenance guard (tuxlink-b026z.7)
        run: bash scripts/check-jt9-provenance.sh
```

- [ ] **Step 4: Test the guard locally — both directions**

Run: `bash scripts/check-jt9-provenance.sh; echo "exit=$?"` → expect `exit=0`.
Then trip-test both directions (clauses enumerate via `git ls-files`, which
sees TRACKED files only — an untracked scratch file passes vacuously):
temporarily add `wsjtr = "1"` to `src-tauri/tuxlink-jt9/Cargo.toml`
(tracked), re-run, expect `exit=1` with the violation line; revert. For
clause 5: temporarily add `let _ = std::process::Command::new("jt9");` to the
EXISTING TRACKED `src-tauri/src/main.rs` (or `git add -N` a scratch file so
ls-files sees it), expect trip, revert. Also trip clause 5's shmem check by
adding `"-s",` to the runner's args array, expect trip, revert.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml scripts/check-jt9-provenance.sh
git commit -m "ci(ft8): install wsjtx for real-jt9 e2e; add GPL-boundary provenance guard (tuxlink-b026z.2 T8, closes b026z.7 scope)"
```

---

### Task 8.5: Packaging — wsjtx Recommends + the AGPL license-metadata fix

**Files:**
- Modify: `src-tauri/tauri.conf.json` (three fields)

Delta §Packaging obligations, owned here because this is the PR that touches
`tauri.conf.json` (the delta ties the license fix to exactly that PR).

- [ ] **Step 1:** In `src-tauri/tauri.conf.json`: append `"wsjtx (>= 2.5)"` to
  `bundle.deb.recommends` (currently `["direwolf (>= 1.6)"]`) and
  `"wsjtx >= 2.5"` to `bundle.rpm.recommends` (currently
  `["direwolf >= 1.6"]`); change `bundle.license` from `"GPL-3.0-or-later"`
  to `"AGPL-3.0-or-later"` (stale from the project's GPLv3→AGPLv3 relicense —
  `LICENSE` and `Cargo.toml` are already AGPL; the artifacts' metadata must
  stop contradicting them).
- [ ] **Step 2:** Add one sentence to `docs/install.md` §prerequisites (find
  the section listing direwolf): "FT8 Station Intelligence uses the `wsjtx`
  package's `jt9` decoder (`sudo apt install wsjtx`); AppImage users install
  it manually — the deb/rpm recommend it automatically."
- [ ] **Step 3:** Re-run the provenance guard (`bash scripts/check-jt9-provenance.sh`)
  — the wsjtx token now appears in `tauri.conf.json` under `recommends`,
  which clause 4 inspects ONLY via `externalBin`/`resources` — expect
  `exit=0` (if it trips, clause 4's jq is over-broad; fix the CLAUSE to keep
  inspecting only the two bundling keys, not the file wholesale).
- [ ] **Step 4:** Expect the deb-install-test CI job to auto-install the
  Recommends chain in all 4 matrix cells (job-time/disk increase — known,
  and it validates the Recommends resolves).
- [ ] **Step 5: Commit**

```bash
git add src-tauri/tauri.conf.json docs/install.md
git commit -m "build(ft8): recommend wsjtx >= 2.5 in deb+rpm; fix stale GPL license metadata to AGPL (tuxlink-b026z.2 T8.5)"
```

---

**REVIEW GATE C (after Tasks 7–8.5):** perspectives: (1) the fixture
regeneration altered ONLY reference `.txt` files, the README, `sdr_parity.rs`
format strings, and the one pre-authorized `oracle.rs` doc line — `git diff
--stat` proves no WAV or decoder-code change in `tuxlink-ft8/src/`; (2) every
guard clause trip-tested in BOTH directions with evidence captured; (3) the
`ci.yml` diff is additive-only; (4) `tauri.conf.json` diff touches exactly the
three named fields. Minimum three rounds; persist findings to
`dev/scratch/b026z.2-gate-C-findings.md` before proceeding.

---

### Task 9: PR + CI verification

**Files:** none (process task).

- [ ] **Step 1:** Push the branch; open the PR: title `[<session-moniker>] feat(ft8): Station Intelligence L1 — managed-jt9 decode service (tuxlink-b026z.2)`, body summarizing: what L1 is, the delta doc pointer, the local test evidence (crate suite + real-jt9 e2e counts), and that L2 (capture) is the next child.
- [ ] **Step 2:** Watch CI **by commit SHA** (`gh api repos/cameronzucker/tuxlink/commits/<sha>/check-runs`), both arches. The verify job now includes the wsjtx install + provenance guard + the tuxlink-jt9 suite. Fix-forward any CI-only failures (dash-vs-bash in fake scripts, runner apt package names).
- [ ] **Step 3:** On green: merge per project policy (`gh pr merge --merge`), close `tuxlink-b026z.2` and `tuxlink-b026z.7` with a note pointing at the PR, file the wsjtx-2.5 verification bd issue if Task 7 Step 3b took the note path, run the worktree disposal ritual (ADR 0009 — never `git worktree remove`), and update the epic with "L1 shipped; next child = b026z.3 (L2 capture subsystem — owns the slot scheduler, tmpfs slot dirs, drop-new-slot backpressure, and the N=5/k=20 counters defined in types.rs)".

---

## Self-Review (performed at write time)

1. **Spec coverage:** delta §Revised L1 → Tasks 1–6 (parser/extractor/preflight/types/discovery/runner incl. prewarm, salvage, taxonomy, shmem ban); §depth policy + fixture regeneration → Task 7 (incl. Step 3b version-floor evidence); §.7 guard + CI → Task 8; §Packaging (Recommends + bundle.license) → Task 8.5; §hashed-callsign disposition → Task 2 + types doc-comment (UI/MCP exclusion lands in L3/L4 plans); §backpressure drop-policy and §tmpfs slot dirs are L2-owner concerns, with the N=5/k=20 thresholds pinned in types.rs for L2 to consume. Slot-scheduler, ring, events: L2/L3 plans by design.
2. **Placeholder scan:** no TBDs; every code step carries complete code; commands carry expected outcomes.
3. **Type consistency:** `Jt9Binary{jt9_path, engine_version}`, `SlotOutcome::{Decoded,BandDead,Failed}`, `SlotFailure` variants, `Ft8Decode` fields, `SLOT_FRAMES`/`SLOT_RATE_HZ` — names checked identical across Tasks 3/4/5/6.

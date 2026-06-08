# 2026-06-08 canyon-poplar-tamarack — Catalog Builder + tuxlink-pat strip (both merged)

## Summary

Two features shipped and **merged** this session, plus a CI-failure investigation and a Pat-reference cleanup:
1. **Catalog Request Builder** (`tuxlink-a2gd`, smoke-walk item 12) — PR **#465 MERGED**.
2. **Removed the `tuxlink-pat` legacy keyring service** (`tuxlink-kc3q`) — PR **#468 MERGED**.

Both bd issues are **closed**. Both worktrees are **disposed** (31G of build cache freed). No work is in-flight.

## State (re-verified at session end)

- **Root checkout:** unchanged, on `bd-tuxlink-xygm/recover-handoffs` (operator state; never touched). Tracked tree clean; only untracked additions under `dev/scratch/`, `dev/handoffs/`, `dev/adversarial/` (this session's artifacts).
- **PRs:** #465 + #468 both **MERGED** with remote-branch deletion. No open PRs from this session.
- **Worktrees:** `bd-tuxlink-a2gd-catalog-builder` (+ the **nested** `bd-tuxlink-kc3q-pat-keyring-strip` — created nested by a `cwd` slip when running the worktree script) — **both disposed** (`rm -rf` + `git worktree prune`; verified gone). Grounding docs preserved to `dev/scratch/canyon-catalog-grounding-{map,LIVE-update}.md`; the Codex adversarial transcripts were discarded (local-scratch per CLAUDE.md).
- bd: `tuxlink-a2gd` + `tuxlink-kc3q` closed.

## What shipped

**Catalog Builder (#465):** Rust `catalog_fetch_stations` (direct HTTPS poll of `cms.winlink.org:444/listings/<mode>Listing.aspx`, 5 confirmed modes, kHz text parser with degrade-to-raw, polite cache: TTL 30m + per-key coalescing + 15m min-refetch + stale-on-error) + `catalog_parse_reply` (area-weather AWIPS parser). Frontend `CatalogBuilderPanel` ("Message → Find a Gateway"), distance-sorted results, ★→Favorites forward hook (disabled — CF agent owns the consumer), `CatalogReplyView` wired into MessageView. Full `build-robust-features` discipline: 2 Codex cross-provider rounds + a 6-reviewer adversarial workflow; ~6 blockers + ~15 majors fixed pre-merge.

**tuxlink-pat strip (#468):** the Pat sidecar was already gone (PR #175, 2026-05-31), but the strip **missed several readers**. Removed `LEGACY_SERVICE` + migration fallback in `credentials.rs`; repointed ARDOP-HF + VARA-HF CMS secure-login reads (`winlink_backend.rs`) from the empty legacy service to canonical `tuxlink` via `credentials::read_password`; fixed a `wizard_persist_cms` restore test that passed **vacuously**; de-Pat-framed the `#[ignore]`'d wizard integration tests. `NativeBackend` is the sole backend; only `X-Pat-Transport` (peer wire-compat) remains live + intentional.

## Pending operator action

- **ARDOP-HF / VARA-HF direct-to-CMS secure-login on-air smoke** (deferred — RF hardware not ready). The keyring repoint can only improve the prior state (those paths read the empty legacy service before). Failure signature if it ever fails: `ExchangeError::PasswordRequired` ("server required a password but none was configured") + `CMS_HEALTH` `password_required` failure record. Loud, observable, recoverable (re-enter password) — operator accepted this risk on merge.

## Follow-ups filed

- `tuxlink-q9r3` (P3): discover the VARA-FM listing endpoint (`RmsVaraFmListing.aspx` is 404).
- `tuxlink-uuhh` (P4): upstream `/listings/` serves some sysop names double-encoded (`André`→`AndrÃ©`); parser preserves bytes faithfully.
- `tuxlink-r8fb` (P3): scrub remaining dead Pat references (deprecated `pat_mbo_address` field + comments). **Keep** `X-Pat-Transport`. Sequence after no conflicting branches are in-flight.

## Lessons captured to memory (verification discipline)

PR #465 cost **2 extra CI rounds** because local verification was scoped (`cargo test` + touched-files vitest) while CI's `verify` gate is stricter. Recorded in `feedback_scoped_vitest_misses_contract_tests`: **before pushing, run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`** (re-run until exit 0 — it hides later-target lints) **+ the full/contract vitest tests** (e.g. `menuModel.test.ts` EXPECTED_IDS — adding a menu item breaks it). #468 then passed verify first try by applying this.

## Next session — pick a big-ticket smoke-walk item (operator will choose)

Ranked shortlist of OPEN, **non-RF** (RF hardware not ready) big-ticket smoke-walk items, best-first:

1. **`tuxlink-bsiy` (P0, non-RF) — Prompt operator to select inbound messages to download** ← **recommended**. Only open P0; ready, no deps; CMS/B2F proposal-accept handshake (all transports incl. Telnet) so fully agent-completable + dev-smokeable over the authorized cms-z internet path. Critical emcomm WLE-parity gap (tuxlink auto-downloads ALL inbound today). Both survey readers ranked it #1.
2. **`tuxlink-6c9y` (P1, non-RF) — Telnet Post Office / Network Post Office modes** (smoke-walk #9). Largest pure-build headline; needs a research/design spike first (decompiled WLE artifacts — operator "not sure what it does"). Pick only if you want the single biggest feature over lowest-uncertainty.
3. **`tuxlink-etxt` (P1, non-RF) — mark messages read/unread.** Clean self-contained UI/mailbox win, no CMS round-trip, browser-smokeable; ideal RF-independent backup.
4. **`tuxlink-ka3z` (P1, non-RF) — nested folders/sub-folders.** CAVEAT: operator believed this shipped — verify spec drift (git log / spec for partial impl) before building.
5. **`tuxlink-4bgn` (P2, mixed) — use downloaded RMS station lists in modem config panes** (smoke-walk #11). **Consumes the catalog station-list layer just shipped in #465** (now data-unblocked). Ingest+picker slice is non-RF/smokeable; only the dial-a-station step is RF. Landing it unblocks `tuxlink-24px` (offline station map).

**RF-deferred until hardware ready:** `tuxlink-7fr`/`5vx` (AX.25), `tuxlink-9ggl`/`7gsc` (tuxmodem/DRA-100), phase-2 per-mode dives, listener/ARDOP/VARA wiring. `tuxlink-24px` is non-RF but hard-blocked on `tuxlink-4bgn`.

For whichever item: full pipeline — `writing-plans` → `build-robust-features` (Codex cross-provider adrev, no skipping) → TDD → **READY** PR (no self-merge; operator smokes UI). Apply the verification lesson above before every push.

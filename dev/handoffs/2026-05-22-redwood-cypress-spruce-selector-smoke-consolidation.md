# Handoff — redwood-cypress-spruce — session-selector smoke + parallel-config consolidation

> **Date:** 2026-05-22 · **Agent:** redwood-cypress-spruce
> **Next-session goal (operator-set):** (1) fix the selector host-application blocker, (2) **consolidate** the parallel AX.25/Bluetooth work into one lineage and kill the two-parallel-configs problem, (3) troubleshoot AX.25 (RX broken), then re-smoke + merge. The operator explicitly does not want two parallel branches/sessions fighting over one config any more.

---

## 1. What shipped this session

**`tuxlink-3pb` — session-type accordion connection selector + per-intent panes. PR #124 (OPEN, ready).**
- Branch `bd-tuxlink-3pb/session-selector`, worktree `worktrees/bd-tuxlink-3pb-session-selector`.
- 8 TDD tasks, subagent-driven, each spec+quality reviewed. Local gates green: `tsc` clean · `vitest` 453 · `cargo --lib` 316.
- Connections sidebar → session-type accordion (Winlink·CMS / Radio-only / Post Office / P2P / Network PO; expand → protocols); AppShell dispatches `{sessionType,protocol}` → Telnet-CMS pane / Packet (cms-gateway|p2p) / stub. CMS host+transport controls relocated out of SettingsPanel into the Telnet-CMS pane.
- **#122 (`tuxlink-3o0`) closed/superseded** — its CMS backend (`config_set_connect`, `resolve_cms_host`, connect-exercise test) is merged into the 3pb branch (merge `540ce97`).
- **Smoke: Telnet path PROVEN** — full CMS exchange to cms-z over plaintext (login + `;PQ`/`;PR` secure-login + `FF`/`FQ`). The selector mechanism works.
- **CAVEAT:** `tuxlink-b2s` — CI `build-linux` path-filter excludes `src/**`, so #124's frontend changes get **no CI**. The real gates are local vitest + the operator browser smoke.

## 2. BLOCKER found in smoke — `tuxlink-ka7` (P1, do NOT merge #124 until fixed)

**Symptom:** operator selected cms-z (config persisted `host=cms-z.winlink.org`, `transport=CmsSsl`), but the connect hit **production** (TLS handshake + `*** Unknown client types … use cms-z.winlink.org` rejection). Since **cms-z has no TLS** (8773 filtered), a cms-z dial *can't* TLS-handshake — so the connect used a different host than the config said.

**Prime hypothesis (UNCONFIRMED — verify first):** `NativeBackend` caches `config: Config` at construction (`winlink_backend.rs:433`, set in `new()` ~481); the connect path (`native_connect` / `resolve_cms_host` ~1205) uses that **cached `self.config`**, and `config_set_connect` writes the **file** but does **not** refresh the live backend. So host/transport selections only take effect after an app **restart**. This is the **same root cause as `tuxlink-p5u`** (*"Packet param changes via UI may need an app restart — backend uses startup config snapshot"*) — and likely related to `tuxlink-eh7` (wizard "no restart-free hand-off"). Timeline fits: pid 399284 (cached cms-z) → cms-z:8773 timeouts; restart → pid 423368 (cached server.winlink.org) → server:8773 handshake+reject; operator set host=cms-z (file) but 423368 kept cached server → attempt 2 hit prod.

**Verify:** does the connect `read_config()` fresh or use `self.config`? Does `config_set_connect` update the live backend? **Fix:** refresh the live backend's config on `config_set_*` (or have the connect re-read config) so UI selections apply without restart. Fix `ka7` + `p5u` together (one architectural fix covers both CMS + packet).

## 3. CMS / TLS reality — CONFIRMED via probes (don't re-derive)

| Endpoint | Reality |
|---|---|
| **cms-z.winlink.org** = `52.37.245.31` | 8772 plaintext **OPEN** (dev, **accepts unregistered** tuxlink SID). 8773 **FILTERED — NO TLS.** |
| **server.winlink.org** = `32.196.219.66` / `52.206.142.38` | 8773 **TLS OPEN**, valid `*.winlink.org` cert — **but REJECTS the unregistered `tuxlink-0.0.1` SID** ("use cms-z"). |

- **`8773` is the CORRECT TLS port — do not change it.** RMS Express also connects `blnSSL: true`.
- **There is NO working dev-TLS path:** cms-z is plaintext-only; prod-TLS rejects unregistered. A *legitimate* TLS smoke is **blocked until `tuxlink-9h8`** (register the tuxlink client SID with Winlink). cms-z over TLS is a dead combo (steer it to Plaintext — see `tuxlink-379`).
- **Correction (I was wrong earlier):** `52.37.245.31` is **cms-z itself**, not a "stale server.winlink.org rotation IP / Tailscale MagicDNS" artifact. The earlier `52.37.245.31:8773` timeouts were the app correctly dialing **cms-z:8773** (no TLS). The bad bd memory has been corrected (`cms-tls-8773-reachability-is-heterogeneous-across-server`). `tuxlink-lbg` connect-hardening note still applies (re-resolve/fall-through on connect timeout), but is lower-priority than ka7.

## 4. The two-parallel-configs problem (operator's main pain) + AX.25

- A parallel session (**`uhc/ax25-tx-timing`**, worktree `worktrees/bd-tuxlink-uhc-ax25-tx-timing`, issues `tuxlink-7fr` / nx2) is adding `KissLinkConfig::Bluetooth{mac}`. Its build wrote `{"Bluetooth":…}` to the **shared** `~/.config/tuxlink/config.json`, which **bricked #124 on open** (`unknown variant Bluetooth` + `deny_unknown_fields` → `read_config` hard-fails). Filed as **`tuxlink-efo`** (P2: read path should degrade, not brick).
- Two branch builds share **one** config file **and** the **one** Vite `:1420` dev port → contamination + collisions. **Operator wants to consolidate** — merge the AX.25/Bluetooth lineage and #124 together so there's one schema/one build/one config.
- **AX.25 "not working":** `tuxlink-4ef` (P1) — *RFCOMM socket transport: TX works but RX broken (relay reply never received)*. Also `tuxlink-sox` (packet transport segment resets to USB instead of persisting Bluetooth) and `tuxlink-2y4` (P0 RADIO-1 packet on-air safety). These live in the parallel session — bring them into the consolidated session.

## 5. bd issue map (the load-bearing ones)

- **`tuxlink-3pb`** — this epic, PR #124 open, **blocked by `ka7`**.
- **`tuxlink-ka7`** (P1, NEW) — selector host/transport not applied without restart. **THE blocker.** Fix with **`tuxlink-p5u`** (same root cause, packet params).
- **`tuxlink-9h8`** (P2, NEW) — register tuxlink client SID with Winlink (gates prod + prod-TLS smoke).
- **`tuxlink-efo`** (P2, NEW) — config-read resilience (unknown packet.link variant bricks app-open).
- **`tuxlink-4ef`** (P1) — AX.25 RFCOMM RX broken (the "AX.25 isn't working").
- **`tuxlink-379`** (P3) — selector polish (active-only dot, missing unit tests) + add cms-z→Plaintext guard.
- **`tuxlink-b2s`** (P2) — CI excludes frontend; #124's CI isn't a real gate.
- **`tuxlink-7fr`** / nx2 — AX.25 + the Bluetooth variant (parallel session).

## 6. Worktree / machine state

- `worktrees/bd-tuxlink-3pb-session-selector` — branch `bd-tuxlink-3pb/session-selector`, pushed (code through `4b86327` + this handoff). Has `node_modules` + a `src-tauri/target/debug` build. A `tauri dev` (pid ~423368) was left running on `:1420` — **stop it** before the follow-up.
- `worktrees/bd-tuxlink-uhc-ax25-tx-timing` — parallel session's branch `bd-tuxlink-uhc/ax25-tx-timing` (Bluetooth AX.25). Not mine; coordinate / consolidate.
- `~/.config/tuxlink/config.json` — SHARED across both builds; currently `host=cms-z.winlink.org, transport=CmsSsl, packet.link=null`.

## 7. Follow-up plan

1. **Consolidate the lineage** — bring `uhc/ax25-tx-timing` (Bluetooth + AX.25) and `bd-tuxlink-3pb/session-selector` together (one branch toward main), so there's a single config schema and a single build. End the two-parallel-configs setup.
2. **Fix the config-snapshot architecture** (`ka7` + `p5u`): `config_set_*` refreshes the live backend (or connect re-reads config) → UI selections apply restart-free. This unblocks both the CMS selector and packet param changes.
3. **Config-read resilience** (`tuxlink-efo`): degrade gracefully on unknown/divergent config (and/or per-worktree `TUXLINK_CONFIG_DIR` for dev) so a Bluetooth-variant config can't brick a non-Bluetooth build.
4. **AX.25 RX** (`tuxlink-4ef`): the RFCOMM transport RX is broken — troubleshoot (operator-run on-air per RADIO-1).
5. Re-smoke #124 (selector now restart-free), then merge. Prod-TLS smoke remains gated on `tuxlink-9h8` (client registration).

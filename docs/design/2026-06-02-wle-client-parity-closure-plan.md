# WLE-client connection-mode parity — closure plan

> **⚠ AMENDED 2026-06-03** — the §2 table and §3.1 "inbound P2P parity = Telnet-P2P only" cross-cutting finding are SUPERSEDED for the non-Telnet transports. WLE doesn't bind an application-layer socket for Packet/ARDOP/VARA-P2P, but the TNC/modem owns the listen at the link/physical layer — inbound sessions arrive via TNC connect-indication or modem `CONNECTED` event, and tuxlink needs the receive side across every P2P-capable transport. See **[`2026-06-03-multi-transport-listener-architecture.md`](2026-06-03-multi-transport-listener-architecture.md)** for the corrected dispositions, 3 new bd issues (tuxlink-3o2o shared listener-arms layer P1 + tuxlink-t9b6 RADIO-1 framing P2 + tuxlink-xnoy VARA listener completion P2), and the re-sequenced Tier 1 listener-foundation roadmap. The affected closure-plan rows are: §2 row 2.4 (tuxlink-inde — was 🟠 defer-listener; **is** 🟢 ship divergence overlay, P1); §2 row 2.7 (tuxlink-dhbl — was 🟠 defer-listener; **is** 🟢 ship LISTEN TRUE + event routing + UI, P1); §2 row 2.8/2.9 (tuxlink-qpqh/do6j — scope expanded to include VARA listener completion).
>
> Date: 2026-06-02 · Agent: thistle-swallow-cedar · bd: tuxlink-a6ic (umbrella) + tuxlink-smpu (this synthesis)
>
> Plan input: APPROVED office-hours design doc at `~/.gstack/projects/cameronzucker-tuxlink/cameronzucker-main-design-20260602-184111.md` (9/10, 2 spec-review iterations).
>
> Audit input: 2026-05-29 capability inventory (`docs/design/2026-05-29-winlink-express-feature-inventory.md`) plus Phase 1 verification (`docs/design/2026-06-02-winlink-express-feature-inventory-verification.md`).
>
> Evidence input: 15 per-mode deep-dive docs at `dev/scratch/winlink-re/findings/<mode>.md` (gitignored; ~500 KB total). Each deep-dive is sourced from the decompiled WLE 11.0.0.0 binary (SHA256 `feda3a92…2332d`) at `dev/scratch/winlink-re/decompiled/RMS Express/RMS_Express/` (258 .cs files, 134 .resx).

## 0. Purpose

This doc sequences the closure of WLE-client connection-mode parity gaps as bd-issue priorities, with per-mode justification grounded in the deep-dive evidence. It also names the operator-decision rows that remain blocking, and the substantive audit corrections the Phase 2 fan-out surfaced.

This doc is the **synthesis spine**. The per-mode deep dives are the authoritative spec input for the build-cycle bd issues that follow this closure plan; this doc decides WHICH issues ship first, WHICH operator-decisions block them, and WHICH 2026-05-29 audit rows need corrective framing before any code is written against them.

## 1. Substantive audit corrections (must propagate downstream)

Three audit rows from `docs/design/2026-05-29-winlink-express-feature-inventory.md` had framings that the Phase 1 verification + Phase 2 deep dives proved wrong. Any future bd-issue spec citing these rows must use the corrected framing.

### 1.1 §10.7 `ConfirmConnection` is post-connect UX, NOT a pre-TX Part 97 gate

The 2026-05-29 audit framed §10.7 as "Confirm-before-connect gate · RADIO-1 alignment — defensive prompt before TX. Worth considering for safety." That framing is wrong on three axes:

- **Timing.** Fires AFTER B2F connection is established, not before TX. Trigger: `B2Protocol.cs:1903-1953`, four-conjunct truth-table including `Globals.blnShowRMSRelayWarning && enmB2SessionType not in {RadioOnly, PostOffice}`.
- **Trigger semantics.** Detects relay/hybrid-degraded gateway via banner-phrase parsing at `B2Protocol.cs:2050-2079` (7 byte-exact `StartsWith`/`Contains` strings including the literal `"will by stored"` TYPO from the binary). It is a deliverability warning, not a transmission consent gate.
- **RADIO-1 conflation.** Per project memory `radio1-governs-tx-not-ui`, Part 97 governs the click on Send/Receive — not UX modals. Treating this as a RADIO-1 gate is the exact escalation pattern that memory forbids.

**Tuxlink proposal:** transform — replicate the banner-phrase parsing + cancel/continue semantics, but replace the modal with a persistent header strip per `inline-ui-no-window-clutter` memory. Explicitly NO RADIO-1 framing in the UI text. (Full deep-dive at `dev/scratch/winlink-re/findings/confirm-connection-post-connect.md`.)

### 1.2 §2.17 "Multi-leg connect script" is Packet-only digipeater chaining, NOT cross-transport try-each-in-order

The 2026-05-29 audit described §2.17 as "Try transports in order until one works." That cross-transport orchestration does not exist in WLE. What `EditConnectScript.cs` actually drives is a Packet-only AX.25 digipeater-chain + BPQ-command script within a single packet session.

- Implementation lives in `PacketWL2KSession.cs` + `PacketP2PSession.cs` only. Zero references in any Telnet / VARA / ARDOP / Pactor session file.
- Per-leg default timeout 60s, overall 300s, overridable inline via `!CONNECTTIME` / `!TOTALTIME`. Failure detection is substring-match on 11 hardcoded fail words + operator extras via `!ABORTWORDS`.
- Cannot distinguish "transport unreachable" from "gateway rejected."
- Storage: per-callsign `<execDir>/<CallsignAndQualifier>/Scripts_WL2K/*.txt` and `Scripts_P2P/*.txt`.

**Tuxlink proposal:**
- The actual WLE feature (Packet digipeater chaining) defers to v0.5+ (after Packet AX.25 ships robustly via tuxlink-7fr).
- The IMAGINED cross-transport try-each-in-order pattern defers indefinitely — conflicts with RADIO-1 per-invocation TX consent and is better served by external cron / shell orchestrator than an in-app auto-fallback loop.

(Full deep-dive at `dev/scratch/winlink-re/findings/multi-leg-connect-script.md`.)

### 1.3 §9.7 "Add Telnet RMS to known-stations" label is materially misleading

The 2026-05-29 audit row 9.7 implied this is a UI for adding alternate CMS endpoints. It is not. `DialogAddTelnetStation` (per `add-telnet-rms-known-stations.md`) adds:

- Telnet-P2P partner stations (record stored to `Telnet P2P Favorites.dat`)
- Or PostOffice / RMS Relay endpoints (record stored to `Telnet PostOffice Favorites.dat`)

Per the deep dive, the only WLE path to dial a non-default CMS endpoint is the `Use RMS Relay` checkbox documented in `client-of-rms-relay.md`. Add-Telnet-RMS-to-known-stations is the P2P/PostOffice favorites manager, not a CMS endpoint editor.

Records are pipe-delimited plaintext (including plaintext passwords) — a credential-handling divergence tuxlink must NOT inherit per `no-disk-creds-default` memory.

**Tuxlink proposal:** replicate the favorites concept; diverge on storage (keyring not plaintext file) and label clearly ("Saved peers / relay endpoints" not "Add Telnet RMS").

(Full deep-dive at `dev/scratch/winlink-re/findings/add-telnet-rms-known-stations.md`.)

## 2. Closure-priority table

The 15 deep-dive subjects map to closure-plan priorities as follows. P1 = blocks the operator's stated pain ("can't accept inbound P2P" + HF unusability). P2 = important parity polish, scheduled after P1 ships. P3 = operator-decision-gated (commercial hardware) or low-audience (deprecated upstream). 🟢 = ship-it; 🟠 = defer-with-rationale; ⚪ = operator-decide.

| § | Mode | bd issue | Priority | Disposition | Justification |
|---|---|---|---|---|---|
| 2.2 | Telnet-P2P (listener) | tuxlink-xehu | **P1** | 🟢 ship | Highest-value closure: this is the operator's "can't accept inbound P2P" pain. Authoritative spec at `findings/telnet-p2p.md` (552 lines, supersedes prior `p2p-telnet.md`). 7 open items but none blocking — they refine downstream test surface. Two security-improvement divergences from WLE: allowed-stations TRUE→FALSE default (restrict by default); station-password INI→keyring. Plus a new divergence on the uppercase-password latent bug. |
| 2.4 | Packet-P2P | tuxlink-inde | **P2** | 🟠 defer/skip-listener | WLE Packet-P2P is OUTBOUND ONLY. No allowed-stations, no auth, no AutoConnect, no listener role. Operator's "can't accept inbound P2P" framing was Telnet-specific; for Packet there is no WLE listener to match. Closure: ship the outbound Packet-P2P dialer (after tuxlink-7fr Packet-AX.25 lands); listener role DROPPED from scope. |
| 2.7 | ARDOP-P2P | tuxlink-dhbl | **P2** | 🟠 defer/skip-listener | Same outbound-only finding as 2.4. WLE has no ARDOP listener, no allowed-stations, no AutoConnect for ARDOP. `Tw*` prefix = Timewave (PK-Link hardware TNC), NOT a parallel ARDOP track — resolves 2026-05-29 Appendix B mystery. Closure: outbound ARDOP-P2P dialer after ARDOP-CMS stabilizes; listener DROPPED. |
| 2.15 | HF best-channel | tuxlink-bajc | **P1** | 🟢 ship (Phase 1 minimal) | Critical for HF usability across ARDOP-CMS / VARA-HF / Pactor / RPR. Phase 1: ground-wave heuristic + call-history scoring + flat-pipe-delimited `RMS Channels.dat` import. Phase 2 (later): Rust VOACAP port replacing `dvoa.dll`. Tuxlink UX: inline panel, no modal. |
| 2.16 | AutoConnect / auto-poll | tuxlink-hfft | **P1** | 🟢 ship Family A; later Family C+D | The "AutoConnect umbrella" is actually 4 distinct features in WLE. Family A (single-shot per-transport interval) is the universal operator-facing surface and ships in Phase 1. Family B (vestigial Pactor-only `AutoConnectSetup.cs`) DROPPED — confirmed dead code per pactor-cms.md. Families C (HF-best-channel-driven) and D (multi-station polling) ship later when HF transports stabilize. |
| 2.3 | Packet-CMS | tuxlink-89oe | **P2 ✏️** | 🟢 partial (in-flight via tuxlink-7fr) | Implementation in-flight as tuxlink-7fr. This deep-dive captures WLE's surface for parity comparison; tuxlink-7fr execution proceeds independently. Notable: Packet AutoConnect INI key covers BOTH directions (P2P + CMS share one value — unique anomaly among Family A transports). |
| 2.8 | VARA HF (client surface) | tuxlink-qpqh | **P2** | 🟢 ship client surface | Client-side TCP control 8300+8301 plaintext loopback to VARA modem; commands MYCALL / PUBLIC / CWID / COMPRESSION / BW / CONNECT / LISTEN / DISCONNECT / ABORT. BUSY display 4-state. Registration READ-ONLY from WLE's view. Uses Family C + Family D (NOT Family A). Modem itself owned separately by ADR 0014. |
| 2.9 | VARA FM (client surface) | tuxlink-do6j | **P2** | 🟢 ship client surface | Same VARA control protocol shape, distinct binary. Channel selector via `RMS VHF Channels.dat`. Family A AutoConnect only (Variant B 10-entry interval enum, not the 13-entry Variant A). Wide/Narrow display-only sourced from gateway record (mode codes 51/52), not operator setting. WLE sends ZERO bandwidth commands. |
| 2.12 | AREDN MESH telnet | tuxlink-esy7 | **P2** | 🟢 ship | Operator-facing label is "Network Post Office." Dialer-only plaintext-B2F default port 8772. Hardcoded `"CMSTelnet\r"` password (per-station password UI-disabled). AREDN discovery via `DialogUpdateMeshNodes` → `http://localnode.local.mesh:8080/cgi-bin/sysinfo.json?services=1`. Latent bug: operator's `Mesh Master Node` setting stored but IGNORED by the hardcoded fetch URL — tuxlink ships the fix as a divergence. |
| 2.13 | Client-of-RMS-Relay | tuxlink-svsb | **P2** | 🟢 ship | Decomposes into 3 dial paths (TelnetSession+Use RMS Relay, TelnetSessionRadioOnly, TelnetMESHSession). All share B2F protocol with one-character `RoutingFlag` discipline (`C`/`R`/`L`) + brittle banner-phrase parsing as relay-self-identification. Resolves Phase 1 OPEN ITEM 3: TelnetSessionRadioOnly differs from non-Radio-only only via `B2SessionType` enum + `RoutingFlag` tagging + 5 banner parsers + 3 log labels. |
| 2.17 | "Multi-leg connect script" | tuxlink-xalo | **P3** | 🟠 defer post-v0.5 | See §1.2 correction. Actual WLE feature (Packet digipeater chaining) defers after AX.25 Packet stabilizes (post-tuxlink-7fr). Imagined cross-transport pattern dropped indefinitely. |
| 9.7 | Add Telnet RMS (favorites) | tuxlink-k6wk | **P2** | 🟢 ship (relabeled) | See §1.3 correction. Replicate the saved-peers / relay-endpoints concept; diverge on storage (keyring); relabel UI ("Saved peers" not "Add Telnet RMS"). Pairs with tuxlink-svsb. |
| 10.7 | ConfirmConnection (post-connect modal) | tuxlink-oxs2 | **P2** | 🟢 ship (as inline-strip) | See §1.1 correction. Replicate the trigger semantics; replace modal with persistent header strip per `inline-ui-no-window-clutter`. Single global INI flag `[Properties] ShowRMSRelayWarning` for suppression. |
| 2.5 | Pactor-CMS | tuxlink-v47l | **P3** | ⚪ operator-decide | SCS hardware ($1000-$4000+, 9 PTC variants). No open-source modem. Family C HF best-channel + Family D AutoPoll. AES forced off for ham users. Pactor 1/2/3/4 caps via `Max Pactor Level`. Operator-decision gate: do we ship hardware integration for a minority HF transport? |
| 2.10 | Robust Packet (RPR) | tuxlink-ejx5 | **P3** | 🟠 drop entirely | Hard-coded sunset warning IN WLE's source: "RPR will be dropped around July 1, 2026." Deep dive recommends tuxlink skip RPR entirely. Closes this bd issue with disposition "DROPPED — upstream sunset." |

## 3. Cross-cutting findings worth flagging upward

Three architecture-level findings from the Phase 2 deep dives that affect the synthesis layer, not any individual mode:

### 3.1 The "inbound P2P" framing needs revision in operator-facing language

The operator's stated pain — *"we can't even accept inbound P2P connections right now"* — assumed P2P-listener semantics across Telnet, Packet, and ARDOP. The decompile evidence:

- **Telnet-P2P** has a real WLE listener (8774 plaintext) + allowed-stations + station-password. This is the closure target for the operator's pain.
- **Packet-P2P** is outbound-only in WLE. AX.25 connect-mode at the TNC layer answers inbound automatically; WLE doesn't add a listener layer above it. "Accepting inbound P2P over Packet" is whatever the TNC chooses to do, not a WLE-side capability.
- **ARDOP-P2P** is outbound-only in WLE. No listener, no allowlist, no auth.

So *"WLE-parity for accepting inbound P2P"* maps cleanly to Telnet-P2P (the tuxlink-xehu work). For Packet-P2P and ARDOP-P2P, "parity with WLE" means **not** adding a listener — those modes have no WLE listener to match.

### 3.2 HF transport story coheres around 4 shared subsystems

Pactor / ARDOP-CMS / VARA-HF / RPR (the 4 HF transports) all share:

- `RMS Channels.dat` for gateway selection (flat-pipe-delimited, refreshable via internet OR radio store-and-forward OR bundled `Def_RMS_Channels_*.zip`).
- `HFChannelSelector` + `BestChannel` engine for "pick the right frequency now" (`PathQuality + ChannelQualityAdjustment` scoring; `dvoa.dll` propagation).
- Family C AutoConnect (HF-best-channel-driven, with `EnabledIfNoInternet` gating).
- Family D AutoPoll (multi-station rotation, Pactor + VARA-HF only — NOT ARDOP, NOT RPR).

When tuxlink ships HF transport closure (any of the four), the **subsystem ships once and applies to all four**. The closure plan sequences them so the subsystem lands once with the highest-priority transport (ARDOP-CMS already-shipped MVP; HF best-channel via tuxlink-bajc; AutoConnect via tuxlink-hfft) and the remaining HF transports (VARA-HF, Pactor, RPR) consume the subsystem rather than duplicating it.

### 3.3 Credential-handling divergences are universal

Six of the 15 deep dives independently surfaced the same anti-pattern: WLE stores credentials (Telnet-P2P station password, PostOffice favorite passwords, RMS Relay station passwords, AREDN node passwords) as **plaintext** in INI sections or pipe-delimited `.dat` files. Per project memory `no-disk-creds-default`, tuxlink defaults to OS keyring; the audit unambiguously confirms WLE's credential handling is the divergence we must NOT inherit.

The closure plan files a single tracking memory rather than per-mode credential bd issues: every transport's bd issue inherits the keyring-not-disk-plaintext default.

## 4. Operator-decision gates (blocking before P3 closure)

| § | Capability | Decision needed | Default-if-no-decision |
|---|---|---|---|
| 2.5 | Pactor-CMS | Ship hardware integration for SCS modems? | Defer indefinitely. Re-open if operator acquires SCS PTC hardware. |
| 2.10 | RPR-CMS | Drop entirely per upstream sunset? | Drop. Close tuxlink-ejx5 with DROPPED disposition. |
| 10.7 | Post-connect modal vs persistent strip | Inline-strip per memory, or modal-as-WLE? | Inline-strip per `inline-ui-no-window-clutter`. |

Operator-decision rows for non-connection-mode features (encryption AES Part-97 carveouts, mariner-audience Catalog/GRIB requests, multi-callsign install, custom folders, etc.) live in the 2026-05-29 audit §13.5 and are out of scope for THIS closure plan; this closure plan covers connection modes only.

## 5. Sequencing (the actual roadmap)

Recommended bd-issue claim order, grouped by what unblocks what:

**Tier 1 — operator's stated pain (P1):**
1. tuxlink-xehu (Telnet-P2P listener) — single most valuable closure; unblocks "accept inbound P2P" stated pain.
2. tuxlink-bajc (HF best-channel selector) — Phase 1 minimal version. Unblocks usable HF for current ARDOP-CMS MVP.
3. tuxlink-hfft (AutoConnect Family A) — universal operator-facing surface. Ships independent of any specific transport.

**Tier 2 — round-out the v0.1 connection-mode story (P2):**
4. tuxlink-inde (Packet-P2P outbound) — pairs with tuxlink-7fr Packet AX.25; listener DROPPED.
5. tuxlink-dhbl (ARDOP-P2P outbound) — depends on tuxlink-ig0 (ARDOP bind-wait); listener DROPPED.
6. tuxlink-svsb (Client-of-RMS-Relay) — small surface, large operational value for served-agency deployments.
7. tuxlink-oxs2 (ConfirmConnection inline-strip) — pairs with tuxlink-svsb; ships together.
8. tuxlink-k6wk (Saved peers / favorites manager) — pairs with tuxlink-xehu (listener) + tuxlink-svsb (relay).
9. tuxlink-esy7 (AREDN MESH telnet) — small surface; latent-bug fix divergence is the value.
10. tuxlink-qpqh (VARA HF client surface) — depends on ADR 0014 modem direction.
11. tuxlink-do6j (VARA FM client surface) — depends on ADR 0014 modem direction.

**Tier 3 — operator-decision gated (P3):**
12. tuxlink-v47l (Pactor-CMS) — hardware-acquisition decision blocking.
13. tuxlink-ejx5 (Robust Packet RPR) — close as DROPPED per upstream sunset.
14. tuxlink-xalo (Multi-leg connect script) — defer post-v0.5; the cross-transport pattern in the audit row is a phantom.

**Operationally:**
- Tier 1 (3 issues) ships first, unblocks the headline operator pain.
- Tier 2 (8 issues) ships in any order, with the pairings above shipping together.
- Tier 3 is gate-blocked; operator decisions land before any code work.

## 6. Trust assessment

The Phase 1 verification doc concluded the 2026-05-29 audit is "trustworthy as Phase-2 input WITH the 10.7 ⚠ correction noted" — 21 ✓ + 2 📦 + 1 ⚠ + 0 ❓ across 24 in-scope rows. Phase 2 deep dives surfaced 2 additional ⚠ corrections (§2.17 mislabeled; §9.7 mislabeled), bringing the total to 3 audit-row mislabels out of 24 rows audited (12.5% drift rate).

**Net assessment of the 2026-05-29 audit:** trustworthy at the capability-level for unshipped-vs-shipped status (the question it was authored to answer); not authoritative for capability semantics — 12.5% of rows had framings the deep dives substantively contradicted. Any future bd-issue spec citing a 2026-05-29 row should also cite the corresponding deep-dive at `dev/scratch/winlink-re/findings/<mode>.md` (gitignored — refer to the citation list above for the binary SHA + decompile provenance).

## 7. References

| Artifact | Path | Tracked? |
|---|---|---|
| This closure plan | `docs/design/2026-06-02-wle-client-parity-closure-plan.md` | ✓ git |
| Phase 1 verification | `docs/design/2026-06-02-winlink-express-feature-inventory-verification.md` | ✓ git |
| 2026-05-29 audit (historical) | `docs/design/2026-05-29-winlink-express-feature-inventory.md` | ✓ git |
| 15 per-mode deep dives | `dev/scratch/winlink-re/findings/*.md` | ❌ gitignored (per CLAUDE.md `dev/scratch/` convention) |
| Decompile cache + MANIFEST | `dev/scratch/winlink-re/decompiled/` | ❌ gitignored |
| Design doc (office-hours) | `~/.gstack/projects/cameronzucker-tuxlink/cameronzucker-main-design-20260602-184111.md` | ❌ home-dir |
| Umbrella bd issue | tuxlink-a6ic | bd / Dolt |

The deep-dive corpus being gitignored is per CLAUDE.md convention for `dev/scratch/`. Each per-mode bd issue (tuxlink-xehu, tuxlink-inde, …) references its corresponding deep-dive path; if a future contributor's local clone doesn't have the cache, they can regenerate via the MANIFEST's documented `ilspycmd` invocation.

---

Agent: thistle-swallow-cedar

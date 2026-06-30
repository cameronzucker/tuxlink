# Handoff — 2026-06-30 — sage-glade-atoll

**Primary purpose of this handoff:** stop cleanly *before* starting a sensitive,
multi-step **Proxmox corosync cluster recovery** so it can be done uninterrupted
in a fresh session. Operator (Cameron) explicitly requested the handoff-first.
Everything else below is session context; the cluster recovery is the resume target.

This session did **no tuxlink source changes** — it was homelab/infra advisory +
GitHub/bd housekeeping. Working tree dirt (README.md, 83 untracked files) is
**pre-existing, not from this session** — do not commit it.

---

## RESUME TARGET — Proxmox corosync cluster recovery (homelab, not tuxlink repo)

**Status:** planned, not started. Root cause confirmed by operator.

**Root cause (confirmed):** corosync. Adding an **m.2 → 2.5 GbE adapter** to one
node renamed network interfaces (PCI re-enumeration shuffles `enoX`/`enpXsY`);
corosync's ring address no longer matched, so that node can't talk cluster traffic
("won't rejoin"). That node is the **anchor with 3 corosync votes**, so its loss
likely left the surviving nodes **inquorate** → `/etc/pve` read-only, can't start
VMs / make changes. Operator has fixed this *class* of self-inflicted issue before;
just hasn't had time (heads-down on Geographica + Tuxlink + work since April).

**Safe recovery sequence (agreed):**
1. **Map state, read-only only.** No changes yet.
2. **Cheap recovery first:** physically pull the m.2 adapter and reboot the anchor.
   If interface rename was the cause, original enumeration returns and it rejoins.
   10-minute test of the whole hypothesis before any config surgery.
3. **Regain quorum** on survivors if needed via `pvecm expected <n>` (safe, reversible).
4. **Backups-first:** stand up **PBS on the Dell OptiPlex 7070** and back up ALL VMs
   *before* moving anything. (Wrinkle: the 7070 is currently a cluster node — evacuate
   its own VMs first.) Once VMs are restorable, all later steps are low-stakes.
5. **Then** migrate VMs to the R730, un-cluster cleanly, finish 7070 → PBS conversion.
   Depending on storage, **backup→restore onto a standalone R730** may beat
   cluster-migrate-then-uncluster (less churn).

**CARDINAL RULES (do not violate):**
- Backups-first. No `pvecm delnode`, no `corosync.conf` edits, no force-quorum
  **until state is mapped**. Cluster surgery on a guess destroys VMs.
- Read-only diagnostics before any mutation.

**OPEN QUESTIONS — get these from operator before touching anything:**
1. **Access path** to the nodes: tailnet? LAN SSH? web UI `:8006`? console only?
   (None of the cluster nodes appeared online on the tailnet this session — only
   pandora/r2-poe/etc. Is pandora on the same LAN as the cluster?)
2. **Node inventory:** count, names, **which is the 3-vote anchor**, **which is the 7070**,
   what each runs.
3. **Storage:** local (LVM-thin/ZFS) vs shared (Ceph/NFS/iSCSI) — decides migrate vs
   backup-restore.
4. **Anchor state:** does it boot? SSH/console reachable? `pvecm status` on a *surviving*
   node + `ip -br a` on the anchor are the two highest-value first reads.

**First read-only commands once access exists:** `pvecm status`,
`corosync-quorumtool -s`, `journalctl -u corosync`, `ip -br a`.

---

## Other session outcomes (context, mostly closed)

### r2-poe (iKOOLCORE R2 POE) — MT7916 Wi-Fi crash, diagnosed
- Host: i3-N305, x86_64, Ubuntu 24.04, kernel 6.17. Card: AsiaRF **AW7916-NPD**
  (MT7916, **mini PCIe**, DBDC, Wi-Fi 6E).
- **Root cause:** NOT ASPM (disabled in BIOS, still crashed). MCU command timeout →
  mt76 self-recovery worker `mt7915_mac_reset_work` → NULL write in
  `mt76_dma_rx_fill_buf` (`error_code 0x0002`) → kernel oops → boot/network wedge.
  Photo trace at `dev/scratch/20260628_175320.jpg`. ASPM-off-and-still-crashing
  points at **bad card / firmware**, not platform.
- **Recovery:** boots fine with card removed; network service was being blocked by it.
  To test safely: blacklist `mt7915e` (boot), then `sudo modprobe mt7915e` while
  `dmesg -wH` watching, so a crash captures instead of wedging.
- **Replacement (operator's ask):** slot is mini PCIe. Options — fresh **AW7916-NPD**
  or **Wallys DR7916** (~$40, in stock at 524wifi; same chipset = also the swap test),
  OR **MT7915 (AW7915-NPD / Wallys DR7915)** for a more mature mt76 path if 6 GHz
  isn't needed. AsiaRF's own site is stale on stock; buy via 524wifi/Amazon.

### Dependabot — resolved
- Operator merged the 5 fully-green bumps.
- Rebased 3 flaky single-check PRs (#963, #957, #956) — failures were infra-flaky.
- Closed superseded **#913** (rmcp→1.4.0).
- Filed **`tuxlink-o98yl`** (P2) rmcp 0.8.5→2.0.0 migration and **`tuxlink-vdzbs`**
  (P3) pbf 4→5 migration. Both are real breaking-API majors, not flaky.
- **#960 (rmcp→2.0):** confirmed real rmcp 2.0 API break (full error inventory in
  `tuxlink-o98yl`: moved types `AnnotateAble/Content/PromptMessageRole/RawResource`,
  `*RequestParam`→`*RequestParams` renames, 8× `#[non_exhaustive]` struct E0639).
  **Recommendation:** do the rmcp 2.0 bump *manually as part of the AI/MCP refactor*
  (rmcp IS the MCP SDK), and **close #960** (operator OK pending — last action proposed).

### Release freeze — re-applied
- **PR #967 merged** → `.github/RELEASE_FREEZE` present on `main`. `release-please.yml`
  now skips on every push to main until the file is deleted. Reason: AI + UI refactor
  would otherwise cut disjointed partial releases. Unfreeze = delete the file in a PR
  once the refactor lands coherently + operator approves a cut.

### Local AI / R730 assessment (advisory only)
- N305 result (gpt-oss-20b "not useful") read as a **quality** ceiling, not speed.
- R730 (256 GB) worth an **on-demand CPU-MoE** experiment: `gpt-oss-120b` (~5B active
  MoE) holds in its RAM and runs at usable CPU speed; a dense 70B would crawl.
  Fill **all memory channels** (only ~6 sticks now = bandwidth left on the table),
  handle **NUMA**. Confirm DDR4 vs DDR3 on open (R730=DDR4; if truly DDR3 it's an R720).
- **Do NOT GPU the R730** — fights the power thesis (it's mothballed *because* of
  ~100 W idle). The Dell GPU kit is a clean path (EPS 8-pin, not PCIe — don't mix
  cables) but irrelevant given power goals.
- The real Elmer-quality lever is **RAG over emcomm docs**, not just a bigger model.
- For emcomm power budget: R730 = base heavy node when grid/gen up; N305 = austere
  field fallback.

---

## Git / tracker state at handoff
- Branch: `bd-tuxlink-ant8s/ardop-connect-fixes` (this handoff committed here).
- Working tree: pre-existing dirt only (README.md modified, 83 untracked) — NOT mine.
- Remote freeze branch `agent-sage-glade-atoll/release-freeze-ai-ui-refactor`: deleted
  (merged via #967).
- bd: `tuxlink-o98yl` (P2) + `tuxlink-vdzbs` (P3) open; persisted via `bd dolt push`.
- No worktrees created this session. No stashes.

Agent: sage-glade-atoll

# Multiple / Tactical Callsigns — Master Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement each phase plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Let one tuxlink install operate under multiple identities — several licensed FCC callsigns plus free-form tactical labels — with on-air impersonation made structurally impossible.

**Architecture:** Capability/handle model. A non-serializable, in-memory `IdentityHandle` is minted only after a credential validates against the OS keyring, and every transmit/listen API requires one — so transmitting under a callsign you haven't authenticated is a compile error. Per-FULL-callsign inbox; shared identity-tagged Sent/Outbox; tactical labels ride under an authenticated parent.

**Tech stack:** Rust (Tauri backend), the OS keyring crate (existing `winlink/credentials`), SQLite search index, React/TS frontend.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md)
**Codex architecture review:** `dev/adversarial/2026-06-10-zjne-identity-arch-codex.md` (gitignored, local-only)
**Companion feature (separate):** tuxlink-73nl — inbox routing rules.

---

## Resolved design decisions (the spec's open questions, now closed)

1. **Keyring verification method.** On switch, retrieve the keyring-stored *activation secret* for the FULL callsign and **constant-time-compare** it to the entered value. The keyring is the trust anchor; no separate verifier/hash store. (`subtle` crate is already a dependency for constant-time compare.)
2. **Activation secret for offline / P2P-only FULL callsigns.** Every FULL identity gets a keyring activation secret **at add-time**. For CMS identities that is the CMS secure-login password (already needed); for offline/P2P-only identities the operator sets a **local activation passphrase**. Activation is uniform: entered value must match the stored secret. Keyring key: `tuxlink-identity-activation:<CALLSIGN>` (distinct from the existing CMS-login key so the two can coexist / be the same value by the operator's choice).
3. **CMS-registration verification for tactical.** Online-verify a tactical address via the Winlink tactical-address lookup; cache `(address → Registered|NotRegistered, checked_unix)` with a **24h TTL**; **fail-closed for CMS** when offline/uncached (P2P/RF unrestricted). The exact lookup (Winlink web API endpoint vs. a CMS protocol query) is confirmed against `dev/scratch/winlink-re/decompiled/.../WL2KInterop` / Winlink API docs **in Phase 5, Task 1** before any network code is written.
4. **Listener-session UI.** Each armed listener shows its bound identity (callsign or tactical label) as a badge in the connections/radio panel. Switching the active identity never mutates an armed listener.

---

## Canonical interface contract (SOURCE OF TRUTH — every phase plan uses these exact names)

All types live in a new `src-tauri/src/identity/` module unless noted. Phase 1 creates them; later phases consume them verbatim.

```rust
// --- addresses & identities ---

/// A validated FCC-format callsign (reuses the existing `validate_identity` loose rules:
/// nonempty, no whitespace, <=32, ASCII-printable).
pub struct Callsign(String);
impl Callsign { pub fn parse(s: &str) -> Result<Self, IdentityError>; pub fn as_str(&self) -> &str; }

/// What an operation operates/addresses AS.
pub enum Address {
    Full(Callsign),     // a licensed FCC callsign
    Tactical(String),   // free-form label, validated <=24 chars, ASCII-printable
}

/// A licensed identity — the security principal. Owns a mailbox + a keyring activation secret.
pub struct FullIdentity {
    pub callsign: Callsign,
    pub label: Option<String>,   // operator-friendly name, e.g. "Club"
    pub has_cms_account: bool,    // true => activation secret is the CMS password; false => local passphrase
    pub cms_registered: bool,     // the callsign itself is CMS-registered (for the callsign's own account)
}

pub enum TacticalCmsState { Unknown, Registered { checked_unix: u64 }, NotRegistered { checked_unix: u64 } }

/// A tactical label operating UNDER a parent FULL identity. No own credential, no own mailbox.
pub struct TacticalIdentity {
    pub label: String,
    pub parent: Callsign,
    pub cms: TacticalCmsState,
}

/// Persisted identity list. NO secrets (secrets live only in the keyring).
pub struct IdentityStore { /* full: Vec<FullIdentity>, tactical: Vec<TacticalIdentity>, last_selected: Option<Address> */ }
impl IdentityStore {
    pub fn load(path: &std::path::Path) -> Result<Self, IdentityError>;
    pub fn save(&self) -> Result<(), IdentityError>;
    pub fn full(&self) -> &[FullIdentity];
    pub fn tactical(&self) -> &[TacticalIdentity];
    pub fn full_by_callsign(&self, c: &Callsign) -> Option<&FullIdentity>;
    pub fn add_full(&mut self, id: FullIdentity) -> Result<(), IdentityError>;
    pub fn add_tactical(&mut self, t: TacticalIdentity) -> Result<(), IdentityError>; // err if parent unknown
    pub fn remove(&mut self, addr: &Address) -> Result<(), IdentityError>;            // err if removing a FULL with tacticals
    pub fn last_selected(&self) -> Option<&Address>;
    pub fn set_last_selected(&mut self, addr: Address);
}

/// In-memory proof of authentication. NON-Serialize/Deserialize. Constructible ONLY inside
/// IdentityService::authenticate after keyring validation. Never persisted.
pub struct IdentityHandle { /* private: full_callsign: Callsign */ }
impl IdentityHandle { pub fn full_callsign(&self) -> &Callsign; }
// NOTE: do NOT derive Serialize/Clone-to-string; a compile-fence test asserts no Serialize impl.

/// The identity an operation runs as.
pub struct SessionIdentity { /* handle: IdentityHandle, address_as: Address */ }
impl SessionIdentity {
    pub fn full(handle: IdentityHandle) -> Self;                                   // address_as = handle.full_callsign
    pub fn tactical(handle: IdentityHandle, label: String) -> Result<Self, IdentityError>; // err unless label registered under handle.full_callsign
    pub fn mycall(&self) -> &Callsign;        // ALWAYS handle.full_callsign — Part 97 station ID on RF
    pub fn address_as(&self) -> &Address;     // Winlink From: full callsign or tactical label
    pub fn handle(&self) -> &IdentityHandle;
}

pub struct IdentityService { /* store: Arc<Mutex<IdentityStore>>, keyring backend */ }
impl IdentityService {
    pub fn authenticate(&self, full: &Callsign, credential: &str) -> Result<IdentityHandle, IdentityError>;
    pub fn set_activation_secret(&self, full: &Callsign, secret: &str) -> Result<(), IdentityError>;
    pub fn clear_activation_secret(&self, full: &Callsign) -> Result<(), IdentityError>;
}

pub enum IdentityError {
    InvalidCallsign(String), InvalidTactical(String),
    UnknownIdentity, ParentNotFound, RemoveHasTacticals,
    NoSecretSet, CredentialMismatch, Keyring(String), Io(String),
}
```

**Keyring keys:** `tuxlink-identity-activation:<CALLSIGN>` (activation secret). The existing CMS-login key is untouched; for a CMS identity the operator may set both to the same value.

### Cross-phase reconciliations (apply during execution — the contract wins over any phase plan)

1. **`IdentityHandle` is `Clone` (cheap, `Arc`-backed inner) but NOT `Serialize`/`Deserialize`.** Phase 1 implements it as `IdentityHandle(Arc<HandleInner>)` where `HandleInner { full_callsign: Callsign }` and neither type derives serde. The `compile_fail` fence asserts no `Serialize` impl — it does NOT forbid `Clone`. Rationale: Phase 6 armed listeners must *capture their own* handle at arm time (`let listener_id = active.session().clone()`), and the active session + N listeners legitimately co-hold the same authority until disarmed; the no-persist guarantee comes from non-`Serialize` + never writing the handle to disk, not from non-`Clone`. This supersedes any "non-`Clone`" assumption or `snapshot_for_listener` workaround in the Phase 1/6 plans.
2. **`IdentityHandle::for_test(callsign)`** — a `#[cfg(test)]`-only constructor Phase 1 provides so Phases 3–7 can build `SessionIdentity` in unit tests without a real keyring. Not compiled into release.
3. **Two independent schema counters, do not conflate:** Phase 2 bumps the **config** schema (`CONFIG_SCHEMA_VERSION`, 1→2). Phase 4 bumps the **search-index** schema (its own counter, for the new `identity_tag` column). They are separate version numbers in separate files; a "v2" config and a "v4" index are not in conflict.
4. **`Address` for tactical** carries the label only; the parent is resolved via `IdentityStore`/the active `SessionIdentity` (a tactical `Address` is valid only under a handle whose `full_callsign` is its registered parent — enforced by `SessionIdentity::tactical`).

**Active-session backend state (Phase 3+):** the Tauri-managed backend holds `Option<SessionIdentity>` (in-memory, never serialized) as the *active default for new operations*, plus the persisted `IdentityStore`. Listeners hold their **own** `SessionIdentity` captured at arm time (Phase 6).

**Tauri command surface (Phase 2 partial, Phase 7 full):**
```
identity_list() -> IdentityListDto                       // full + tactical, with needs_auth + cms badge flags; no secrets
identity_add_full(callsign, label, has_cms_account, activation_secret)
identity_add_tactical(label, parent)
identity_remove(address)
identity_switch(address, credential) -> Result<()>       // authenticate -> set active SessionIdentity
identity_active() -> Option<ActiveIdentityDto>           // { mycall, address_as, is_tactical }
```

---

## Phase sequence, dependencies, and session breaks

Each phase is its own plan file + bd issue, and must end green (its tests + `cargo clippy --all-targets -D warnings`, plus `tsc`/scoped vitest for UI). Dependency order: **1 → 2 → 3 → {4, 5, 6} → 7**.

| Phase | bd | Plan file | Size | Session break guidance |
|---|---|---|---|---|
| 1. Identity core | tuxlink-d4wp | `…-phase-1-identity-core.md` | M | **One session.** Pure module + unit tests; no wiring. |
| 2. Config + migration | tuxlink-7iy2 | `…-phase-2-config-migration.md` | M | **One session.** Migration is fiddly; commit the migration test first. |
| 3. Handle threading | tuxlink-0063 | `…-phase-3-handle-threading.md` | **L** | **Two sessions.** Break after the CMS/telnet path is converted + green; do ARDOP/VARA/packet RF paths in the second. Biggest blast radius. |
| 4. Per-FULL mailbox | tuxlink-2ns7 | `…-phase-4-mailbox.md` | M-L | **One session** (optionally break inbox-namespacing vs Sent/Outbox-tagging). Builds on 9efs/mzm4. |
| 5. CMS gating (tactical) | tuxlink-tseu | `…-phase-5-cms-gating.md` | S-M | **One session.** Task 1 (confirm the lookup endpoint) gates the rest. |
| 6. Re-auth + listeners | tuxlink-5ekg | `…-phase-6-reauth-listeners.md` | M | **One session.** |
| 7. UI | tuxlink-noa0 | `…-phase-7-ui.md` | **L** | **Two sessions.** Break Tauri commands + DTOs (session 1) vs ribbon switcher + inline unlock + mailbox filter + listener badges (session 2). |

**Estimated ~9–10 implementation sessions.** Phases 4/5/6 can run in any order (all depend only on Phase 3) and could be parallelized across sessions/worktrees if desired; Phase 7 needs all three.

**Per-phase definition of done:** all phase tasks complete, the phase's tests green, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` clean, `tsc --noEmit` + scoped vitest green for any frontend, CI green on both arches, PR merged, bd phase issue closed.

---

## Self-review (master)

- **Spec coverage:** every spec section maps to a phase — identity model + handle → P1; config/migration → P2; transmit/listen enforcement + Part 97 mycall → P3; mailbox model → P4; CMS gating → P5; re-auth + listeners → P6; UI + Tauri commands → P7; routing rules → separate tuxlink-73nl. No spec requirement is unassigned.
- **Type consistency:** the interface contract above is the single source of truth; phase plans must use these exact names (`IdentityHandle.full_callsign()`, `SessionIdentity::mycall()`, `IdentityService::authenticate`, `IdentityError` variants, the keyring key format).
- **Open questions:** all four resolved above; no phase plan should contain a "TBD" for them.

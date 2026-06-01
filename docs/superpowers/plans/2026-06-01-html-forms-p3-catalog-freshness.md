# HTML Forms — Phase 3: Catalog freshness + form-aware reply + draft library generalization

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out the HTML Forms surface. After P3, the bundled WLE catalog stays fresh against winlink.org via in-app refresh, the operator's saved-slot library works on every native form, replies to received forms use WLE `_SendReply.0` templates (the operationally-correct path), and any P2-deferred polish (PDF export for ICS-309, map widget for Position, hot-reload for custom-forms dir, operator-override custom-forms dir) lands.

**Architecture:** A new `forms::updater` module owns the catalog freshness loop: download → hash-verify → extract to a temp dir → atomic swap with the current snapshot → rollback on failure. Form-aware reply extends the existing `replyActions.ts` to consult the WLE `_SendReply.0` template for the source message's form id and prefill via the catalog. The `FormDraftLibrary` from P2 generalizes by removing the Check-In-only registration; the slot dropdown becomes a shared component used by every native form.

**Tech stack:** Rust (reqwest for HTTP, sha2 for hash, zip for unpack) / TypeScript / React / Vitest / Tauri.

**Branch / worktree:** Execute in a fresh worktree owned by bd `tuxlink-4w8u`. Create before Task 1:

```bash
python3 .claude/scripts/new_tuxlink_worktree.py \
  --slug p3-catalog-freshness \
  --issue tuxlink-4w8u \
  --base main \
  --moniker <your-session-moniker>
```

Branch will be `bd-tuxlink-4w8u/p3-catalog-freshness`. All file paths in this plan are relative to that worktree.

**Design reference:** [`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`](../specs/2026-05-31-html-forms-full-parity-design.md) §6 Phase 3 (deliverables), §7 (components), §9 (error handling), §11 (testing strategy), §14 (risks).

**Depends on P1 (`tuxlink-ytya`) + P2 (`tuxlink-hnkn`)**. P3 PR can stack on P2 if P2 isn't merged when P3 starts — `bd dep add tuxlink-4w8u tuxlink-hnkn` was recorded at planning-sprint time.

**Operator decisions to verify at plan-start:**
- Are P2's deferred items (PDF library, map widget) being picked up here or deferred again? Read `bd show tuxlink-hnkn`'s P2-decision-lock summary first.
- Custom-forms hot-reload: live (inotify/FSWatcher) or restart-to-pick-up?
  Design §13 Q3 lists this as deferred to per-phase plan; we lock it here.

**Adversarial review:** Two cross-cutting Codex rounds:
1. `forms::updater` (Task 1) — security-critical (downloads + unpacks operator-trustless content from the network) and atomicity-critical (atomic swap with rollback)
2. Form-aware reply (Task 4) — touches the receive→reply pipeline that PR #177 made plain-text

**Browser-smoke gate:** Per `feedback_browser_smoke_before_ship`. PR stays open for operator smoke.

---

## Task 0: Lock the residual decisions + check P2 disposition

**Files / artifacts:**
- Read: `bd show tuxlink-hnkn` (P2 decision-lock summary in notes)
- Write: `dev/scratch/2026-06-XX-p3-decision-lock.md` (gitignored)

- [ ] **Step 1: Read P2's decisions.**

```bash
bd show tuxlink-hnkn | grep -A 20 "decision\|Notes" | head -30
```

For each open question:
- **PDF library**: if P2 chose one, this task is a no-op for PDF. If P2 deferred, set scope as: ship the PDF library this phase. Default to **typst** if no other preference (lighter dep than wkhtmltopdf, modern toolchain).
- **Map widget**: if P2 chose one, no-op. If P2 deferred, set scope as: ship a Leaflet integration with offline tile pack default to the operator's geographic region (TBD: how to pick the tile pack — default to a small global low-zoom set?).
- **Form draft library scope**: P2 default was Check-In only; this phase generalizes regardless.

- [ ] **Step 2: Lock custom-forms hot-reload decision.**

Options:
- **Live (inotify via `notify` crate)**: nice UX, custom forms appear in CatalogBrowser without restart. Adds `notify = "6"` dep + a tokio task.
- **Restart-to-pick-up**: no new dep, but operators must close-and-reopen the compose window after dropping a new form.

Default: **live**. The `notify` crate is well-maintained and the UX win is meaningful (operators iterating on a custom form want the feedback loop fast). If operator explicitly prefers restart-to-pick-up via the bd notes, defer to that.

- [ ] **Step 3: Write the decision record.**

```bash
cat > dev/scratch/2026-06-XX-p3-decision-lock.md <<'EOF'
# P3 Decision Lock — captured <date>

## Inherited from P2
- PDF library: [chosen-in-P2 / deferred-to-P3 → <choice this phase>]
- Map widget: [chosen-in-P2 / deferred-to-P3 → <choice this phase>]
- Operator-override custom-forms dir: [deferred-to-P3 → ship in Task 6]

## New this phase
- Custom-forms hot-reload: [live (notify) / restart-to-pick-up]

## Implications
- Task 1: catalog updater always lands
- Task 3: FormDraftLibrary generalization always lands
- Task 4: form-aware reply always lands
- Task 5: PDF (if needed); map (if needed)
- Task 6: custom-forms dir override settings UI
EOF
```

---

## Task 1: `forms::updater` — winlink.org catalog freshness

**Files:**
- New: `src-tauri/src/forms/updater.rs`
- Update: `src-tauri/Cargo.toml` (add `zip = "2"`, `sha2 = "0.10"`, `tempfile` is already present)
- Update: `src-tauri/src/ui_commands.rs` (commands: `forms_check_for_updates`, `forms_apply_update`)

Downloads the WLE Standard Forms zip from winlink.org, verifies integrity (sha256 against a known-good hash OR fall back to a sane sanity-check), extracts to a tempdir, then atomic-swaps with the bundled snapshot location. Rollback on any failure.

**Atomicity discipline:** Per spec §6 P3, "atomic snapshot swap with rollback on bad zip." The implementation pattern:

```
1. Download to /tmp/tuxlink-forms-update-<token>/download.zip
2. Verify hash (configurable: pin a known-good sha or skip-verify)
3. Extract to /tmp/tuxlink-forms-update-<token>/extracted/
4. Sanity check the extract (folder structure matches Standard_Forms convention)
5. Atomically swap:
     mv <forms-data-dir>/Standard_Forms <forms-data-dir>/Standard_Forms.bak.<ts>
     mv /tmp/.../extracted <forms-data-dir>/Standard_Forms
   (If step 2 fails, leave .bak; on next launch, prune .bak older than 7d.)
6. On failure of step 5b, restore from .bak.
```

The "live" snapshot location moves from `resources/wle-forms/` (read-only,
bundled into the binary) to a writable data dir like
`~/.local/share/tuxlink/forms/standard/`. The bundle stays as the **seed**:
on first launch, copy `resources/wle-forms/Standard_Forms/` → data-dir if
the data-dir is empty. Then the updater only writes to data-dir.

This implies `forms::wle_templates::bundle_root_for_app` needs to point at
the data-dir, not the resource dir, with the seed-from-resource step
happening before the first call. **Lock this seeding step at the top of
Task 1** before adding the updater proper.

- [ ] **Step 1: Implement seed-from-resource.**

In `src-tauri/src/forms/wle_templates.rs` (or a new sibling
`forms::storage`), add:

```rust
/// Ensure the writable data dir is seeded from the read-only resource bundle
/// on first launch. Idempotent: if the data dir already has Standard_Forms,
/// no-op.
pub fn ensure_seeded(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().data_dir().map_err(|e| e.to_string())?
        .join("tuxlink/forms/standard");
    if data_dir.join("Standard_Forms").exists() {
        return Ok(data_dir.join("Standard_Forms"));
    }
    let resource = app.path()
        .resolve("resources/wle-forms/Standard_Forms",
                 tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    copy_dir_recursive(&resource, &data_dir.join("Standard_Forms"))
        .map_err(|e| e.to_string())?;
    Ok(data_dir.join("Standard_Forms"))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dst_path)?;
        } else {
            std::fs::copy(&path, &dst_path)?;
        }
    }
    Ok(())
}
```

Then update `bundle_root_for_app` to call `ensure_seeded` instead of
resolving the resource dir directly. The resource dir stays as the seed.

- [ ] **Step 2: Test scaffold for the updater.**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Fake a winlink.org HTTP response with a known-good zip + sha.
    /// (mockito is already a dev-dependency.)
    #[tokio::test]
    async fn fetch_and_verify_writes_zip_to_tempdir() {
        let mut server = mockito::Server::new_async().await;
        let fake_zip = build_fake_standard_forms_zip();
        let sha = sha256(&fake_zip);
        let _m = server.mock("GET", "/standard-forms-latest.zip")
            .with_status(200)
            .with_body(fake_zip.clone())
            .create_async()
            .await;

        let result = fetch_and_verify(&format!("{}/standard-forms-latest.zip", server.url()), Some(&sha)).await.unwrap();
        // result is a path to the downloaded zip
        let body = std::fs::read(&result).unwrap();
        assert_eq!(body, fake_zip);
    }

    #[tokio::test]
    async fn fetch_with_wrong_hash_rejects() {
        let mut server = mockito::Server::new_async().await;
        let _m = server.mock("GET", "/x.zip")
            .with_status(200)
            .with_body(b"not a zip" as &[u8])
            .create_async()
            .await;
        let r = fetch_and_verify(&format!("{}/x.zip", server.url()), Some("0000000000000000000000000000000000000000000000000000000000000000")).await;
        assert!(r.is_err(), "wrong hash must reject");
    }

    #[test]
    fn atomic_swap_succeeds_on_clean_extract() { /* … */ }

    #[test]
    fn atomic_swap_rolls_back_on_extract_failure() {
        // Build a bogus "extract" dir that fails sanity (no Standard_Forms
        // subdir); verify the live dir is unchanged after rollback.
    }

    #[test]
    fn http_failure_leaves_live_dir_unchanged() { /* … */ }
}

fn build_fake_standard_forms_zip() -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        zip.start_file("Standard_Forms/ICS Forms/ICS213_Initial.html", Default::default()).unwrap();
        zip.write_all(b"<html>fake</html>").unwrap();
        zip.start_file("Standard_Forms/VERSION", Default::default()).unwrap();
        zip.write_all(b"v9.9.9-fake\n").unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}
```

- [ ] **Step 3: Implement the updater.**

```rust
//! WLE Standard Forms catalog updater.
//!
//! Downloads the winlink.org zip, verifies integrity (sha256 against pinned
//! known-good hash if provided), extracts to a tempdir, atomic-swaps with
//! the live snapshot. Rollback on any failure leaves the live dir
//! unchanged.
//!
//! Design reference: §6 P3, §9 (error handling), §14 (risks).

use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

pub const WLE_STANDARD_FORMS_URL: &str = "https://downloads.winlink.org/User%20Programs/Standard_Forms.zip";

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    #[error("download failed: {0}")] Download(String),
    #[error("hash mismatch: expected {expected}, got {got}")] HashMismatch { expected: String, got: String },
    #[error("extract failed: {0}")] Extract(String),
    #[error("sanity check failed: {0}")] Sanity(String),
    #[error("swap failed: {0}")] Swap(String),
    #[error("io: {0}")] Io(#[from] std::io::Error),
}

pub async fn fetch_and_verify(
    url: &str,
    expected_sha256: Option<&str>,
) -> Result<PathBuf, UpdateError> {
    let body = reqwest::get(url).await
        .map_err(|e| UpdateError::Download(e.to_string()))?
        .bytes().await
        .map_err(|e| UpdateError::Download(e.to_string()))?
        .to_vec();
    if let Some(expected) = expected_sha256 {
        let got = format!("{:x}", Sha256::digest(&body));
        if got != expected {
            return Err(UpdateError::HashMismatch { expected: expected.into(), got });
        }
    }
    let td = tempfile::Builder::new().prefix("tuxlink-forms-").tempdir()?;
    let path = td.into_path().join("download.zip");
    std::fs::write(&path, &body)?;
    Ok(path)
}

pub fn extract_and_sanitize(zip_path: &Path) -> Result<PathBuf, UpdateError> {
    let extract_root = zip_path.parent().unwrap().join("extracted");
    std::fs::create_dir_all(&extract_root)?;

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| UpdateError::Extract(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| UpdateError::Extract(e.to_string()))?;
        let name = entry.enclosed_name()
            .ok_or_else(|| UpdateError::Extract(format!("bad zip entry name at index {i}")))?;
        // Strip the top-level "Standard_Forms/" prefix if present; tuxlink
        // expects the contents directly under our Standard_Forms dir.
        let out_path = extract_root.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    // Sanity: expect a `Standard_Forms` subdir
    let standard_forms = extract_root.join("Standard_Forms");
    if !standard_forms.exists() {
        return Err(UpdateError::Sanity(
            "extracted zip lacks Standard_Forms subdir".into(),
        ));
    }
    // Sanity: at least one *.html file present
    let any_html = walkdir::WalkDir::new(&standard_forms)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().map(|x| x == "html").unwrap_or(false));
    if !any_html {
        return Err(UpdateError::Sanity("no HTML templates found".into()));
    }
    Ok(standard_forms)
}

pub fn atomic_swap(
    live_dir: &Path,
    extracted_dir: &Path,
) -> Result<(), UpdateError> {
    let parent = live_dir.parent()
        .ok_or_else(|| UpdateError::Swap("no parent".into()))?;
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup = parent.join(format!("Standard_Forms.bak.{ts}"));
    if live_dir.exists() {
        std::fs::rename(live_dir, &backup).map_err(|e| UpdateError::Swap(e.to_string()))?;
    }
    // Attempt the move. On failure, restore.
    if let Err(e) = std::fs::rename(extracted_dir, live_dir) {
        if backup.exists() {
            // Best-effort restore. If THIS fails too, the operator has a
            // .bak dir and an empty Standard_Forms; surface both errors.
            let _ = std::fs::rename(&backup, live_dir);
        }
        return Err(UpdateError::Swap(e.to_string()));
    }
    Ok(())
}

/// Prune .bak directories older than 7 days.
pub fn prune_backups(parent: &Path) -> std::io::Result<()> {
    if !parent.exists() { return Ok(()); }
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(7 * 24 * 60 * 60);
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("Standard_Forms.bak.") { continue; }
        let modified = entry.metadata()?.modified()?;
        if modified < cutoff {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Tauri command surface.**

```rust
#[tauri::command]
pub async fn forms_check_for_updates(
    app: tauri::AppHandle,
) -> Result<bool, String> {
    // Cheap HEAD request to compare against pinned VERSION; returns Ok(true)
    // when a newer version is available, Ok(false) otherwise. Implementation
    // depends on whether winlink.org exposes a versioning convention; if
    // not, fall back to "always offer update; let the operator confirm."
    Ok(true)
}

#[tauri::command]
pub async fn forms_apply_update(
    app: tauri::AppHandle,
) -> Result<(), String> {
    let live = crate::forms::wle_templates::ensure_seeded(&app).map_err(|e| e.to_string())?;
    let zip = crate::forms::updater::fetch_and_verify(
        crate::forms::updater::WLE_STANDARD_FORMS_URL,
        None,  // hash pin TBD; see §Operator decisions
    ).await.map_err(|e| e.to_string())?;
    let extracted = crate::forms::updater::extract_and_sanitize(&zip).map_err(|e| e.to_string())?;
    crate::forms::updater::atomic_swap(&live, &extracted).map_err(|e| e.to_string())?;
    let _ = crate::forms::updater::prune_backups(live.parent().unwrap());
    Ok(())
}
```

- [ ] **Step 5: Verify + commit + Codex adrev.**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib forms::updater 2>&1 | tail -10
git add src-tauri/src/forms/updater.rs src-tauri/src/forms/wle_templates.rs \
        src-tauri/src/forms/mod.rs src-tauri/src/ui_commands.rs src-tauri/Cargo.toml
git commit -m "feat(forms): updater module — winlink.org catalog freshness (Task 1)

Spec §6 P3. Downloads the WLE Standard Forms zip, verifies sha256 (when
pinned), extracts to tempdir, atomic-swaps with the live data-dir
snapshot. Rollback on any failure. .bak dirs pruned after 7 days.

Seeds the writable data-dir snapshot from the bundled resource on first
launch; bundle is the seed, data-dir is the live surface.

Refs: bd tuxlink-4w8u P3 Task 1; spec §6 P3 + §9.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

Codex adrev — security-critical (network-fetched untrusted content):

```bash
cat > /tmp/codex-prompt-updater.txt <<'EOF'
Adversarial review of forms::updater against origin/main.
Run `git diff origin/main -- src-tauri/src/forms/updater.rs src-tauri/src/forms/wle_templates.rs` for the diff.

Attack angles:
1. Zip-slip: a zip entry with `..` in its enclosed_name() escapes the
   extract root. zip::read::enclosed_name() is supposed to reject these
   — verify it does, and add explicit canonicalization if not.
2. Symlinks in the zip: do we honor symlink entries? If so, can a
   symlink point to / or /etc?
3. Resource exhaustion: extracting a zip-bomb (compressed 1KB → 10GB)
   succeeds? Cap the per-entry decompressed size and the total extract
   size.
4. Hash-pin: when `expected_sha256` is None we accept ANY zip. Is this
   the right default? Should we fail-closed in production?
5. Atomic swap: the rename(live, backup) + rename(extracted, live) is
   NOT atomic (a crash between the two leaves no live dir). What does
   the next-launch behavior look like in that state — does ensure_seeded
   re-seed from the bundle?
6. Backup pruning: .bak dirs may contain user-relevant overrides if the
   operator dropped custom forms into the live tree manually. Pruning
   them after 7d is destructive; verify our docs warn against direct
   edits to the live tree.
7. HTTP: reqwest::get() uses the default TLS stack; is certificate
   validation on? CT/HSTS? Are we vulnerable to a downgrade attack on
   first run?

Read: src-tauri/src/forms/updater.rs

Output P0/P1/P2/P3 severity.
EOF
cat /tmp/codex-prompt-updater.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-XX-p3-updater-codex.md
```

Apply ALL P0 + P1 findings before the PR opens (high-blast-radius module).

---

## Task 2: In-app "Refresh forms" UI

**Files:**
- New: `src/settings/FormsRefreshPanel.tsx` (or extend an existing Settings panel)
- Update: `src/shell/AppShell.tsx` or wherever Settings live (route the new panel into the existing Settings)
- New: `src/settings/FormsRefreshPanel.test.tsx`

Confirmation dialog → "Check for updates" → status display ("Update available", "Up to date", "Error") → confirm-to-apply button. Disables the button while an update is in progress (axum + extract are I/O-heavy; the UI should not let the operator double-click).

- [ ] **Step 1: Test scaffold.**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { FormsRefreshPanel } from './FormsRefreshPanel';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'forms_check_for_updates') return true;  // update available
    if (cmd === 'forms_apply_update') {
      await new Promise(r => setTimeout(r, 10));
      return null;
    }
    return null;
  }),
}));

describe('<FormsRefreshPanel>', () => {
  it('shows "check" button initially', () => {
    render(<FormsRefreshPanel />);
    expect(screen.getByRole('button', { name: /check for updates/i })).toBeInTheDocument();
  });

  it('after check, shows "Update available" + "Apply" button', async () => {
    render(<FormsRefreshPanel />);
    fireEvent.click(screen.getByRole('button', { name: /check/i }));
    expect(await screen.findByText(/update available/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /apply/i })).toBeInTheDocument();
  });

  it('"Apply" button is disabled while update is running', async () => {
    render(<FormsRefreshPanel />);
    fireEvent.click(screen.getByRole('button', { name: /check/i }));
    const apply = await screen.findByRole('button', { name: /apply/i });
    fireEvent.click(apply);
    expect(apply).toBeDisabled();
    await waitFor(() => expect(screen.getByText(/applied/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Implement + commit.** (CSS-blind vitest is fine; the panel is informational, no complex interactivity.)

---

## Task 3: Generalize `FormDraftLibrary` to all native forms

**Files:**
- Update: `src/compose/CheckInForm.tsx` (extract the slot-dropdown into a shared component)
- New: `src/compose/FormDraftLibraryPanel.tsx` — the shared dropdown + save-as-slot panel
- Update: `src/compose/PositionFormV2.tsx`, `Ics309FormV2.tsx`, `src/forms/ics213/Ics213Form.tsx`, `src/forms/bulletin/BulletinForm.tsx` — mount the panel
- (No backend changes; the SQLite schema from P2 already supports any form_id)

The panel takes a `formId` prop and a `currentPayload: Record<string, string>` so it knows what to save. The panel emits `onApply(payload)` so the host form can splice the slot's payload into its state.

- [ ] **Step 1: Extract the panel.**

```tsx
// src/compose/FormDraftLibraryPanel.tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Slot {
  slotId: string;
  formId: string;
  label: string;
  payload: Record<string, string>;
  createdAt: string;
  updatedAt: string;
}

interface Props {
  formId: string;
  currentPayload: Record<string, string>;
  onApply: (payload: Record<string, string>) => void;
}

export function FormDraftLibraryPanel({ formId, currentPayload, onApply }: Props) {
  const [slots, setSlots] = useState<Slot[]>([]);
  // … standard list + save + delete UI …
}
```

- [ ] **Step 2: Mount in each native form.**

Verbatim pattern (apply to each of PositionFormV2, Ics309FormV2, Ics213Form, BulletinForm, CheckInForm):

```tsx
<FormDraftLibraryPanel
  formId="<this-form's-id>"
  currentPayload={...the-form's-field-values-map...}
  onApply={(payload) => {
    // splice into the form's state setters one field at a time
  }}
/>
```

- [ ] **Step 3: Tests + commit.**

Each touched form gets a vitest case: render the form, mock the
`form_draft_library_list` IPC to return one slot, click the slot,
assert the form's fields update.

```bash
pnpm exec vitest run src/compose 2>&1 | tail -10
git add src/compose/FormDraftLibraryPanel.tsx src/compose/PositionFormV2.tsx \
        src/compose/Ics309FormV2.tsx src/compose/CheckInForm.tsx \
        src/forms/ics213/Ics213Form.tsx src/forms/bulletin/BulletinForm.tsx
git commit -m "feat(forms): generalize FormDraftLibrary across all native forms (P3)

P2 shipped slot persistence as a Check-In-only feature. P3 extracts the
slot dropdown / save-as-slot UI into a shared FormDraftLibraryPanel and
mounts it in every native form (Position, ICS-309, ICS-213, Bulletin,
Check-In). Backend schema is unchanged — the SQLite table from P2
already supports per-form-id slots.

Refs: bd tuxlink-4w8u P3 Task 3; spec §6 P3 + §13 Q3.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Form-aware reply via WLE `_SendReply.0` templates

**Files:**
- Update: `src/mailbox/replyActions.ts` — detect a `_SendReply.0` template for the source message's form id and route to the form's reply mode
- Update: `src-tauri/src/forms/wle_templates.rs` (or wherever the catalog lookup lives) — add `find_reply_template_for_form_id`
- Tests: extend `src/mailbox/replyActions.test.ts`

Spec §6 P3: "Form-aware reply via WLE `_SendReply.0` templates (currently plain-text per PR #177; this is the operationally-correct path to support per-form replies operators expect from WLE)."

WLE convention: for a form `Foo_Initial.html`, the reply template is `Foo_SendReply.0.html` in the same folder. tuxlink should:
1. When the operator clicks "Reply with form" on a received form message:
   - Look up the source message's form_id
   - Check for a `_SendReply.0` template adjacent
   - If found → route into the appropriate form mode (native if registered with a Form; webview otherwise)
   - If absent → fall through to plain-text reply (current behavior)

- [ ] **Step 1: Backend: `find_reply_template_for_form_id`.**

```rust
pub fn find_reply_template_for_form_id(
    catalog: &[Template],
    source_form_id: &str,
) -> Option<&Template> {
    // Reply templates follow the WLE convention: a form_id like
    // "ICS213_Initial" has its reply at "ICS213_SendReply.0" in the
    // same folder. Search the catalog.
    let reply_id = source_form_id
        .strip_suffix("_Initial")
        .map(|stem| format!("{stem}_SendReply.0"))
        .unwrap_or_else(|| format!("{source_form_id}_SendReply.0"));
    catalog.iter().find(|t| t.id == reply_id)
}
```

Test cases:

```rust
#[test]
fn finds_reply_template_for_ics213() {
    let catalog = vec![
        mk_template("ICS213_Initial", "ICS Forms"),
        mk_template("ICS213_SendReply.0", "ICS Forms"),
        mk_template("Bulletin_Initial", "General"),
    ];
    let result = find_reply_template_for_form_id(&catalog, "ICS213_Initial");
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, "ICS213_SendReply.0");
}

#[test]
fn returns_none_for_form_without_reply_template() {
    let catalog = vec![mk_template("Bulletin_Initial", "General")];
    let result = find_reply_template_for_form_id(&catalog, "Bulletin_Initial");
    assert!(result.is_none());
}

#[test]
fn handles_form_ids_without_Initial_suffix() {
    let catalog = vec![
        mk_template("Position_Report", "General"),
        mk_template("Position_Report_SendReply.0", "General"),
    ];
    let result = find_reply_template_for_form_id(&catalog, "Position_Report");
    assert!(result.is_some());
}
```

- [ ] **Step 2: Tauri command: `find_form_reply_template(form_id)`.**

Returns `Option<Template>` so React can decide how to route.

- [ ] **Step 3: Update `replyActions.ts` to consult the catalog.**

```tsx
async function buildReplyDraft(message: ParsedMessage, mode: ReplyMode): Promise<Draft> {
  if (mode === 'reply-with-form' && message.formId) {
    const replyTemplate = await invoke<Template | null>(
      'find_form_reply_template',
      { formId: message.formId },
    );
    if (replyTemplate) {
      // Route to form mode (native if registered, webview otherwise)
      return { kind: 'form-reply', formId: replyTemplate.id, sourceMid: message.id };
    }
  }
  // existing plain-text fallback path
  return buildPlainReply(message);
}
```

- [ ] **Step 4: Test + commit.**

```bash
pnpm exec vitest run src/mailbox/replyActions 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml test --lib forms::wle_templates 2>&1 | tail -5
git add src/mailbox/replyActions.ts src/mailbox/replyActions.test.ts \
        src-tauri/src/forms/wle_templates.rs src-tauri/src/ui_commands.rs
git commit -m "feat(forms): form-aware reply via WLE _SendReply.0 templates

Spec §6 P3. When the operator replies to a received form message,
tuxlink looks up the source form's _SendReply.0 template in the
catalog. If present → route into form-reply mode (native or webview);
if absent → fall through to the existing plain-text reply (PR #177).

Refs: bd tuxlink-4w8u P3 Task 4; spec §6 P3.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

Codex adrev focus: form_id parsing edge cases (`_Initial` not at end; double underscore; null form_id), catalog mutation under live update (Task 1 swap + Task 4 lookup racing).

---

## Task 5: PDF export for ICS-309 (if P2 deferred)

**Files (conditional):**
- New: `src-tauri/src/forms/pdf.rs` (typst-based renderer)
- Update: `src/compose/Ics309FormV2.tsx` — wire the existing "Download PDF" button to the new IPC

Skip this task if P2 picked + shipped a PDF library. If P2 deferred:

- [ ] **Step 1: Add typst Cargo deps.**

```toml
typst = { version = "0.13", default-features = false }
typst-pdf = "0.13"
typst-render = "0.13"
```

- [ ] **Step 2: Write a minimal typst template for ICS-309.**

Embedded as a Rust string constant; renders an ICS-309 table from a list of rows.

- [ ] **Step 3: Tauri command: `forms_render_ics309_pdf(rows) -> Vec<u8>`.**

Returns PDF bytes; React side wraps them in a Blob and triggers download.

- [ ] **Step 4: Verify + commit.**

---

## Task 6: Custom-forms drop-dir hot-reload + operator-override

**Files:**
- Update: `src-tauri/src/forms/wle_templates.rs` (or new `forms::watcher.rs`)
- Update: `src-tauri/Cargo.toml` (add `notify = "6"` if hot-reload chosen)
- New: `src/settings/CustomFormsPanel.tsx` (operator-override UI)
- Update: `src-tauri/src/config.rs` (persist the custom-forms-dir override)

P1 shipped the default custom-forms-dir at `~/.local/share/tuxlink/forms/custom/`. P3 makes it operator-overridable via Settings and (per Task 0 default) live-reloads on file change.

- [ ] **Step 1: Settings UI for the override.**

A folder-picker input in `CustomFormsPanel`. Persists the chosen path via `config_set_custom_forms_dir`.

- [ ] **Step 2: Hot-reload via notify (if chosen in Task 0).**

```rust
use notify::{Watcher, RecursiveMode, Event};

pub fn spawn_watcher(
    custom_dir: PathBuf,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res { let _ = tx.send(event); }
    }).map_err(|e| e.to_string())?;
    watcher.watch(&custom_dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    tokio::spawn(async move {
        while let Some(_) = rx.recv().await {
            // Debounce: many filesystems emit multiple events per save.
            // 500ms is plenty.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = app.emit("forms-catalog-updated", ());
        }
    });
    // Watcher must live for the lifetime of the app — stash in tauri::State.
    Ok(())
}
```

React side: `CatalogBrowser` subscribes to `forms-catalog-updated` and refetches.

- [ ] **Step 3: Test + commit.**

Watcher integration test: drop a file into a tempdir under watch, verify
the event fires within 1s. (Use tokio::time::timeout.)

---

## Task 7: Map widget for Position (if P2 deferred)

**Files (conditional):**
- New: `src/compose/PositionFormV2Map.tsx` (Leaflet wrapper)
- Update: `package.json` — `leaflet`, `@types/leaflet`
- Update: `src/compose/PositionFormV2.tsx` — mount the map widget into the existing `.position-form-v2__map` slot

If Task 0 chose Leaflet:

- [ ] Add `leaflet@1.9` + `@types/leaflet@1.9` to package.json deps
- [ ] Bundle a small offline tile pack (TBD — discuss with operator if a region-specific pack is needed; default to a global low-zoom 256-tile pack ~2MB)
- [ ] Map renders centered on the current grid; clicking a point updates the form's grid input via lat/lon → Maidenhead conversion
- [ ] Tests + commit

---

## Task 8: End-to-end smoke + Codex full-diff adrev + PR open

- [ ] **Step 1: Full sweep.**

```bash
pnpm exec vitest run 2>&1 | tail -5
cargo --manifest-path src-tauri/Cargo.toml test --lib 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml clippy --all-targets -- -D warnings 2>&1 | tail -5
pnpm exec tsc --noEmit 2>&1 | tail -5
```

- [ ] **Step 2: Codex full-diff adrev.**

Cross-cutting concerns the per-module rounds can't see:

1. Refresh + draft library + form-aware-reply lifecycle: an in-progress reply mid-refresh — does the reply still resolve against the old or new catalog snapshot? Could a mid-flight reply fail when its source-message's form_id disappears from the new snapshot?
2. Custom-form hot-reload + open compose window: a custom form is open in CatalogBrowser when the file changes — is the open form replaced, refused, or unchanged?
3. Settings + draft library: changing the custom-forms-dir via Settings while a saved slot references a form-id that's gone from the new dir — what happens?
4. PDF generation + memory: large ICS-309 logs (1000+ rows) — is typst rendering blocking, OOM-prone?

- [ ] **Step 3: Push + open PR + bd update.**

PR body MUST include the operator browser-smoke checklist:

```
1. pnpm tauri dev → Settings → "Forms" panel → "Check for updates" →
   "Update available" status appears.
2. Click "Apply"; status shows "Applying…" then "Up to date." Inspect
   ~/.local/share/tuxlink/forms/standard/ — new Standard_Forms tree
   present; .bak.<ts> tree from the previous live snapshot.
3. Compose → Position Report → save with Group="My Net". Compose →
   ICS-213 → notice the saved-slot dropdown is present (generalized
   from P2 Check-In-only).
4. Reply to a received ICS-213 message → "Reply with form" → opens
   into ICS213_SendReply.0 mode (native if registered; webview
   otherwise).
5. Drop a custom HTML form into ~/.local/share/tuxlink/forms/custom/
   while CatalogBrowser is open → it appears within 1s (hot-reload).
6. Settings → "Custom forms directory" → change to /tmp/mytests/forms;
   restart compose → CatalogBrowser now reads from /tmp/mytests/forms.
7. ICS-309 → Download PDF → PDF opens correctly.
```

---

## Acceptance criteria

- [ ] `forms::updater` lands with download + verify + extract + atomic swap + rollback + backup pruning; security-focused Codex round applied
- [ ] In-app Refresh forms UI ships; CSS-blind tests green
- [ ] FormDraftLibrary generalized across all native forms; each form's tests carry the new panel mount + slot-apply case
- [ ] Form-aware reply lands; `_SendReply.0` lookup logic tested in Rust
- [ ] PDF export shipped (or deferred to a follow-up bd if Task 0 said skip)
- [ ] Custom-forms hot-reload via notify (if chosen) + operator-override UI ship
- [ ] Map widget shipped (or skipped per Task 0)
- [ ] `pnpm vitest run` + `cargo test --lib` + `cargo clippy -D warnings` + `pnpm tsc --noEmit` all green
- [ ] PR opened with operator browser-smoke checklist
- [ ] bd `tuxlink-4w8u` updated with PR URL + Codex disposition; status in_progress until operator merge
- [ ] HTML Forms full-parity surface complete: bundled snapshot stays fresh, custom forms work end-to-end, replies use WLE templates, draft library is consistent across forms

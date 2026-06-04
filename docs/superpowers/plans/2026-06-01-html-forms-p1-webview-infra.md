# HTML Forms — Phase 1: Webview infrastructure + bundled WLE templates + CatalogBrowser

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the webview-rendering foundation for HTML Forms so every WLE Standard Form (and operator-dropped custom forms) opens in a tuxlink-skinned child webview, submits through a hardened loopback HTTP server, and emits a wire-format-correct `OutboundMessage` via the existing PR #177 serialize path. From this phase onward operators have full WLE catalog coverage in tuxlink.

**Architecture:** A lazy `axum` server bound to `127.0.0.1:0` (kernel-assigned port) serves the WLE template HTML with `{FormServer}` / `{FormPort}` / `{FormFolder}` substitutions baked in, plus the tuxlink CSS skin and a diagnostic fallback JS bridge. The form's native submit posts to `http://127.0.0.1:<port>/submit/<token>`; the parsed body goes through the existing `forms::serialize` → `OutboundMessage` pipeline. The child webview gets a scoped Tauri capability (`forms-webview.json`) with **no IPC** — the only channel back to tuxlink is the loopback HTTP server. The form picker is replaced by a hierarchical `CatalogBrowser` that mixes bundled + custom forms.

**Tech stack:** Rust (axum 0.7, multer 3, tower, tokio) / TypeScript / React / Vitest / Tauri 2 (child webviews).

**Branch / worktree:** This plan executes in a fresh worktree owned by bd `tuxlink-ytya`. Create it before Task 1 via:

```bash
python3 .claude/scripts/new_tuxlink_worktree.py \
  --slug p1-webview-infra \
  --issue tuxlink-ytya \
  --base main \
  --moniker <your-session-moniker>
```

Branch will be `bd-tuxlink-ytya/p1-webview-infra`. All file paths in this plan are relative to that worktree at `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ytya-p1-webview-infra/` (substitute your actual path).

**Design reference:** [`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`](../specs/2026-05-31-html-forms-full-parity-design.md) §5 (architecture), §6 Phase 1, §7 (components), §8.2 / §8.3 (data flow), §9 (errors), §10 (security), §11 (testing).

**Adversarial-review discipline:** Per `feedback_codex_post_subagent_review` + `feedback_no_carveout_on_cross_provider_adrev`, every Rust module commit gets a Codex adrev round before the PR ships. Captures: `dev/adversarial/2026-06-01-p1-<module>-codex.md` (gitignored). The http_server module is the highest-risk surface — minimum one dedicated round on it.

**Browser-smoke gate:** Per `feedback_browser_smoke_before_ship`, the PR sits open for operator browser smoke before merge. The smoke walk-through is enumerated in the final task; the PR must include it in the body.

**Naming-collision note:** This plan creates `forms::wle_templates` (the new module), NOT `forms::templates`. The existing `forms::templates` directory holds per-native-form Rust module template strings (ICS-213, Bulletin, etc., from PR #177) and we do not rename it here. Calling the new module `wle_templates` avoids ambiguity and keeps PR #177's surface untouched.

**Naming for the new Rust modules:**

| Spec name | Plan name | Why |
|---|---|---|
| `forms::templates` (per spec §7) | `forms::wle_templates` | Disambiguates from existing `forms::templates` per-native-form module |
| `forms::skin` | `forms::skin` | No conflict |
| `forms::http_server` | `forms::http_server` | No conflict |
| `forms::multipart` | `forms::multipart` | No conflict |

---

## Task 0: WLE Standard Forms snapshot pre-flight

**Files / artifacts:**
- Read (research only): operator's external `dev/scratch/2026-06-01-wle-snapshot-recon.md` if present (from overnight item G)
- Write: `dev/scratch/2026-06-01-wle-snapshot-decision.md` (gitignored decision record)
- Write: `src-tauri/resources/wle-forms/VERSION` (committed; records the snapshot pin)
- Write: `src-tauri/resources/wle-forms/SHA256SUMS` (committed; integrity check)

Bundle the WLE Standard Forms snapshot into the binary at a pinned version. The snapshot lives under `src-tauri/resources/wle-forms/` and is included via `tauri.conf.json::bundle::resources`.

- [ ] **Step 1: Read the recon findings (overnight item G).**

If `dev/scratch/2026-06-01-wle-snapshot-recon.md` exists (committed by overnight item G), read it for: URL, file size, zip structure verification. If absent, the operator's note in the bd-`tuxlink-ytya` issue or PR #186 body should name the snapshot source. If neither is present, STOP and escalate — Task 0 is the only decision-locking task and a guessed snapshot voids the rest of the plan.

Expected source (from Pat source code as the canonical reference): the Standard Forms zip distributed via winlink.org's downloads page. Pat downloads it at install-time; tuxlink bundles it.

- [ ] **Step 2: Write the decision record.**

```bash
cat > dev/scratch/2026-06-01-wle-snapshot-decision.md <<'EOF'
# WLE Standard Forms snapshot — pinned version for v0.10.0 bundle

## Source URL
<paste the URL from recon doc>

## Snapshot timestamp
<paste from HTTP Last-Modified or filename version>

## Local archive
<absolute path on the executor's disk>

## SHA-256
$(sha256sum <path> | awk '{print $1}')

## Bundle size
$(du -h <path>)

## Bundle budget (set in design §14)
≤ 20 MB total binary growth post-bundle.

## Decision
[Accept / Defer-with-link / Reject].

EOF
```

- [ ] **Step 3: Extract + filter the snapshot.**

The full Standard Forms zip contains content tuxlink should NOT bundle (legacy/empty folders, .DS_Store, OS artifacts, Pat-specific README). Extract to `src-tauri/resources/wle-forms/Standard_Forms/`, then prune:

```bash
mkdir -p src-tauri/resources/wle-forms/Standard_Forms/
unzip -o <local-zip-path> -d src-tauri/resources/wle-forms/Standard_Forms/
# prune OS / VCS / readme artifacts
find src-tauri/resources/wle-forms/Standard_Forms/ \
  \( -name '.DS_Store' -o -name 'Thumbs.db' -o -name '*.bak' -o -name '*.log' \) \
  -delete
# record the snapshot version pin
echo "<version-or-date>" > src-tauri/resources/wle-forms/VERSION
# hash every retained file for integrity check
(cd src-tauri/resources/wle-forms/Standard_Forms && \
 find . -type f -print0 | sort -z | xargs -0 sha256sum) \
  > src-tauri/resources/wle-forms/SHA256SUMS
```

- [ ] **Step 4: Verify bundle budget.**

```bash
du -sh src-tauri/resources/wle-forms/
```

Expected: ≤ 20 MB. If exceeded, STOP — design §14 mandates a smaller "core" snapshot + on-first-online-run download for the rest. That alternate flow is in scope only if budget is breached.

- [ ] **Step 5: Add the resource to `tauri.conf.json`.**

```bash
# Locate the bundle.resources array and append the wle-forms tree.
# Edit src-tauri/tauri.conf.json — find the "bundle" key, then "resources":
#   "resources": [..., "resources/wle-forms/**/*"]
```

- [ ] **Step 6: Commit the bundle + decision record.**

```bash
git add src-tauri/resources/wle-forms src-tauri/tauri.conf.json dev/scratch/2026-06-01-wle-snapshot-decision.md
git commit -m "chore(forms): bundle WLE Standard Forms snapshot (vX.Y.Z, NN MB)

Pin: <version>
Source: <URL>
Sha256: <hash-of-source-zip>
Files: <N>
Bundle size: <NN MB>
Budget: ≤ 20 MB (design §14) — OK.

Refs: bd tuxlink-ytya P1.

Agent: <your-session-moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

NOTE: `dev/scratch/` is `.gitignore`d; the decision record is recovered via `git stash` or copy. Use `tar czf .claude/worktree-archives/wle-snapshot-decision-$(date -u +%Y%m%dT%H%M%SZ).tar.gz dev/scratch/2026-06-01-wle-snapshot-decision.md` to archive it.

---

## Task 1: Add Rust dependencies

**Files:** `src-tauri/Cargo.toml`

Add the axum stack + multipart parser. Choose **multer 3** for multipart (battle-tested, used by axum's own examples).

- [ ] **Step 1: Append the deps.**

```toml
# In [dependencies], insert near reqwest/tokio (the runtime peers):
axum = "0.7"                      # NEW (tuxlink-ytya P1) — lazy loopback HTTP server for HTML Forms webview path; design §5.3
tower = { version = "0.5", features = ["util"] }  # NEW (tuxlink-ytya P1) — axum router + service composition
tower-http = { version = "0.6", features = ["fs", "cors"] }  # NEW (tuxlink-ytya P1) — static-file serving for the WLE asset subtree
mime = "0.3"                      # NEW (tuxlink-ytya P1) — Content-Type construction for HTTP responses
multer = "3"                      # NEW (tuxlink-ytya P1) — multipart/form-data parser preserving repeated names per design §5.3
percent-encoding = "2"            # NEW (tuxlink-ytya P1) — manual urlencoded body parse where multer doesn't apply
walkdir = "2"                     # NEW (tuxlink-ytya P1) — custom-forms directory recursive enumeration
```

- [ ] **Step 2: Verify `cargo check` is clean.**

```bash
cargo --manifest-path src-tauri/Cargo.toml check 2>&1 | tail -20
```

Expected: `Finished ... profile [unoptimized + debuginfo] target(s) in NN.NNs`. New deps download + compile; first run is slow.

- [ ] **Step 3: Commit.**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(deps): add axum/multer/walkdir for HTML Forms webview infra

P1 prerequisite — see plan §Task 1 for rationale per dep.

Refs: bd tuxlink-ytya P1.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Spike — minimal webview loads a loopback form

**Files:**
- New: `src-tauri/src/forms/spike.rs` (deleted in Task 6)
- Temporary IPC: a `forms_spike_start` Tauri command that returns the loopback URL

Per spec §14 "WebKitGTK + axum + Tauri 2 child-webview pattern is novel" — front-load a spike before building the full module so a blocker is caught at hour 1 not hour 8. This task is **discardable**; it gets deleted when `forms::http_server` lands in Task 6.

- [ ] **Step 1: Write the spike module.**

Create `src-tauri/src/forms/spike.rs`:

```rust
//! TEMPORARY (deleted in Task 6 of the P1 plan). Minimal axum-on-127.0.0.1:0
//! that serves one hardcoded HTML page so we can confirm the
//! tauri::WebviewWindowBuilder + WebKitGTK + lazy-port pattern works on the
//! pandora target before building the full forms::http_server.

use std::net::SocketAddr;
use tokio::net::TcpListener;
use axum::{routing::get, Router, response::Html};

pub async fn spawn() -> Result<u16, String> {
    let app = Router::new().route("/", get(root));
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .map_err(|e| format!("bind 127.0.0.1:0 failed: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?
        .port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(port)
}

async fn root() -> Html<&'static str> {
    Html(r#"<!doctype html>
<html><head><title>tuxlink spike</title></head>
<body style="background:#222;color:#ccc;font-family:sans-serif;padding:2em;">
<h1>Spike OK</h1>
<p>This page is being served by axum bound to 127.0.0.1:&lt;random&gt; and
loaded into a Tauri child webview.</p>
<form method="POST" action="/submit"><button>Submit (no-op)</button></form>
</body></html>"#)
}
```

- [ ] **Step 2: Wire the IPC command.**

In `src-tauri/src/lib.rs` or `src-tauri/src/ui_commands.rs` (wherever the existing Tauri command macros live), add:

```rust
pub mod forms { pub mod spike; }

#[tauri::command]
pub async fn forms_spike_start() -> Result<String, String> {
    let port = crate::forms::spike::spawn().await?;
    Ok(format!("http://127.0.0.1:{port}/"))
}
```

Register it in the Tauri builder's `.invoke_handler(...)`.

- [ ] **Step 3: Spike from the React side.**

In `src/App.tsx` or a temporary `src/SpikePage.tsx`, add a button:

```tsx
<button onClick={async () => {
  const url = await invoke<string>('forms_spike_start');
  const webview = new WebviewWindow('spike', { url });
  webview.once('tauri://error', (e) => console.error('spike error', e));
}}>Open spike</button>
```

- [ ] **Step 4: Operator-runs `pnpm tauri dev`.**

Have the operator (or self-run if operator has authorized) click the button. Expected: a child window opens showing "Spike OK". If the child webview fails to bind, blank-screens, or hits CSP errors, STOP — the spike has revealed a Tauri/WebKitGTK blocker and the spec §14 fallback (Option A) needs revisiting.

- [ ] **Step 5: Commit the spike (mark explicitly disposable in commit body).**

```bash
git add src-tauri/src/forms/spike.rs src-tauri/src/lib.rs src/App.tsx  # or wherever the spike entry-point lives
git commit -m "chore(spike): minimal axum-loopback-loaded-in-child-webview spike

Discardable spike per spec §14 risk-mitigation. Confirms the WebKitGTK +
axum + Tauri 2 child-webview pattern is viable on pandora before Task 6
builds the real forms::http_server. forms::spike + the forms_spike_start
command + the App.tsx button are deleted by Task 6.

Refs: bd tuxlink-ytya P1.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `forms::wle_templates` — bundled + custom enumeration

**Files:**
- New: `src-tauri/src/forms/wle_templates.rs`
- New: `src-tauri/src/forms/wle_templates_test.rs` (unit tests; embedded `#[cfg(test)] mod tests`)
- Update: `src-tauri/src/forms/mod.rs` (add `pub mod wle_templates;`)

Walks the bundled `resources/wle-forms/Standard_Forms/` (resolved at runtime via Tauri's resource API) plus the custom-forms directory (default `~/.local/share/tuxlink/forms/custom/`, operator-overridable) and returns a flat catalog of every form template (HTML file) with: id, display name, source kind (`Bundled` | `Custom`), folder path (for `{FormFolder}` substitution at Task 6), and absolute path on disk.

**Design discipline (TDD):** every public function gets a failing test BEFORE its body lands.

- [ ] **Step 1: Test scaffold — list returns bundled forms.**

In `src-tauri/src/forms/wle_templates.rs`, append at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a fake "bundle" tree under a tempdir mirroring the on-disk
    /// shape of the Standard_Forms snapshot so list() has something
    /// recognizable to walk without dragging in 20MB of real WLE assets
    /// at test time.
    fn fake_bundle(td: &TempDir) -> PathBuf {
        let root = td.path().join("Standard_Forms");
        std::fs::create_dir_all(root.join("ICS Forms")).unwrap();
        std::fs::write(
            root.join("ICS Forms/ICS213_Initial.html"),
            "<html><!-- ICS-213 --></html>",
        ).unwrap();
        std::fs::write(
            root.join("ICS Forms/ICS213_Reply.html"),
            "<html><!-- ICS-213 reply --></html>",
        ).unwrap();
        std::fs::create_dir_all(root.join("ARC Forms")).unwrap();
        std::fs::write(
            root.join("ARC Forms/ARC213.html"),
            "<html><!-- ARC213 --></html>",
        ).unwrap();
        root
    }

    fn fake_custom(td: &TempDir) -> PathBuf {
        let root = td.path().join("custom");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("MyCustom.html"), "<html><!-- custom --></html>").unwrap();
        root
    }

    #[test]
    fn list_includes_bundled_forms_with_folder_metadata() {
        let td = TempDir::new().unwrap();
        let bundle = fake_bundle(&td);
        let custom = fake_custom(&td);
        let cat = list(&bundle, Some(&custom)).unwrap();
        let by_id: std::collections::HashMap<_, _> = cat.iter()
            .map(|t| (t.id.clone(), t)).collect();
        assert!(by_id.contains_key("ICS213_Initial"));
        assert!(by_id.contains_key("ICS213_Reply"));
        assert!(by_id.contains_key("ARC213"));
        assert!(by_id.contains_key("MyCustom"));
        let ics = by_id.get("ICS213_Initial").unwrap();
        assert_eq!(ics.source, TemplateSource::Bundled);
        assert_eq!(ics.folder, "ICS Forms");
        let custom_t = by_id.get("MyCustom").unwrap();
        assert_eq!(custom_t.source, TemplateSource::Custom);
        // Custom forms can live at the root of the custom dir → folder = ""
        assert_eq!(custom_t.folder, "");
    }

    #[test]
    fn list_returns_empty_when_paths_dont_exist() {
        let td = TempDir::new().unwrap();
        let cat = list(&td.path().join("nope"), None).unwrap();
        assert!(cat.is_empty());
    }

    #[test]
    fn list_skips_non_html_files() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("ICS Forms")).unwrap();
        std::fs::write(root.join("ICS Forms/notes.txt"), "ignore me").unwrap();
        std::fs::write(root.join("ICS Forms/Real.html"), "<html></html>").unwrap();
        let cat = list(root, None).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].id, "Real");
    }

    #[test]
    fn list_custom_overrides_bundled_by_id() {
        // If the operator drops a custom form with the same id as a bundled
        // one, the custom takes precedence (per spec §6 P1 — custom forms
        // pickup-able). The catalog returns ONE entry per id.
        let td = TempDir::new().unwrap();
        let bundle = fake_bundle(&td);
        let custom = fake_custom(&td);
        // Drop an override
        std::fs::write(custom.join("ICS213_Initial.html"), "<html><!-- OPERATOR OVERRIDE --></html>").unwrap();
        let cat = list(&bundle, Some(&custom)).unwrap();
        let ics: Vec<_> = cat.iter().filter(|t| t.id == "ICS213_Initial").collect();
        assert_eq!(ics.len(), 1, "exactly one entry expected after override");
        assert_eq!(ics[0].source, TemplateSource::Custom);
    }
}
```

Expected (BEFORE you write the impl): `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink wle_templates` → compile error (no `list` / `TemplateSource` / `Template` in scope).

- [ ] **Step 2: Implement the public surface.**

```rust
//! WLE Standard Forms + custom-form template enumeration.
//!
//! Walks the bundled snapshot tree (extracted from `resources/wle-forms/`
//! at build time per Task 0) and the operator's custom-forms directory
//! (default `~/.local/share/tuxlink/forms/custom/`) and returns a flat
//! catalog of every HTML template with id, folder, source kind, and path.
//!
//! Custom forms with the same `id` as a bundled form WIN (the operator's
//! file shadows the bundled one).
//!
//! The catalog is consumed by:
//! - `forms::http_server` to look up the file when a webview requests
//!   `/forms/<token>/<id>`
//! - React `CatalogBrowser` to render the picker tree (via the
//!   `forms_list_catalog` Tauri command)

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateSource {
    Bundled,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Template {
    /// Form id derived from the filename stem (e.g. `ICS213_Initial.html` → `ICS213_Initial`).
    pub id: String,
    /// Display label — for now, same as `id`; spec §13 may revisit.
    pub label: String,
    /// Folder path relative to the bundle / custom root, used for
    /// `{FormFolder}` substitution in the WLE template.
    pub folder: String,
    pub source: TemplateSource,
    pub path: PathBuf,
}

pub fn list(bundle_root: &Path, custom_root: Option<&Path>) -> std::io::Result<Vec<Template>> {
    let mut templates = walk_html(bundle_root, TemplateSource::Bundled);
    if let Some(custom) = custom_root {
        let custom_list = walk_html(custom, TemplateSource::Custom);
        // Custom wins: index by id, overwrite any bundled entry of the same id.
        let mut by_id: std::collections::HashMap<String, Template> = templates
            .drain(..)
            .map(|t| (t.id.clone(), t))
            .collect();
        for t in custom_list {
            by_id.insert(t.id.clone(), t);
        }
        templates = by_id.into_values().collect();
        templates.sort_by(|a, b| a.id.cmp(&b.id));
    }
    Ok(templates)
}

fn walk_html(root: &Path, source: TemplateSource) -> Vec<Template> {
    if !root.exists() {
        return Vec::new();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map(|x| x == "html").unwrap_or(false))
        .map(|e| {
            let path = e.path().to_path_buf();
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let folder = path
                .strip_prefix(root)
                .ok()
                .and_then(|p| p.parent())
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_string();
            Template {
                id: id.clone(),
                label: id,
                folder,
                source: source.clone(),
                path,
            }
        })
        .collect()
}
```

- [ ] **Step 3: Resolve `bundle_root` at runtime.**

The bundled snapshot lives in the Tauri resources dir. Add a thin wrapper that resolves it:

```rust
pub fn bundle_root_for_app(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    app.path()
        .resolve("resources/wle-forms/Standard_Forms", tauri::path::BaseDirectory::Resource)
}

pub fn custom_root_for_app(app: &tauri::AppHandle) -> PathBuf {
    // Default: ~/.local/share/tuxlink/forms/custom/
    // Operator-overridable in P3 via Settings (out of scope here).
    app.path()
        .data_dir()
        .map(|d| d.join("tuxlink/forms/custom"))
        .unwrap_or_else(|_| PathBuf::from("forms/custom"))
}
```

- [ ] **Step 4: Verify tests green.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib wle_templates 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed; 0 failed; 0 ignored`.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/forms/wle_templates.rs src-tauri/src/forms/mod.rs
git commit -m "feat(forms): wle_templates module — bundled + custom enumeration

P1 first module. Walks the bundled WLE Standard Forms snapshot
(resources/wle-forms/Standard_Forms/) plus the operator's custom-forms
directory and returns a flat catalog (id, label, folder, source, path).
Custom forms with the same id as a bundled form override the bundled
entry. Tested via tempdir fixtures (no 20MB asset drag at test time).

Refs: bd tuxlink-ytya P1; spec §7 / §8.2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Codex adrev.**

```bash
cat > /tmp/codex-prompt-templates.txt <<'EOF'
Adversarial review of the diff against origin/main in this worktree for the
forms::wle_templates module. Run `git diff origin/main -- src-tauri/src/forms/wle_templates.rs` to see the changes.

Attack angles:
1. Path-traversal: can a custom-form file outside `custom_root` (via symlink,
   `..`, or absolute path in the dir) be enumerated?
2. ID collision: are non-HTML files / hidden files mis-identified as templates?
3. Resource-leak: does WalkDir handle permission-denied / inaccessible dirs
   gracefully or panic?
4. Custom-overrides-bundled: is the override semantics correct under empty
   string ids (filename ".html" or "" stem)?
5. Cross-platform path-separator: does `folder` come out clean on Linux
   (`ICS Forms`)? What if the resource dir contains backslash-named files?

Read these files: src-tauri/src/forms/wle_templates.rs

Output findings as a markdown block at the end.
EOF
cat /tmp/codex-prompt-templates.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-01-p1-wle-templates-codex.md
wc -l dev/adversarial/2026-06-01-p1-wle-templates-codex.md  # expect 1500-4000
```

Apply P0/P1 findings as follow-up commits. P2/P3 → file bd issues.

---

## Task 4: `forms::skin` — tuxlink CSS skin asset

**Files:**
- New: `src-tauri/src/forms/skin.rs`

Generates the CSS skin string injected via `<link rel=stylesheet href=/skin.css>` by the http_server in Task 6. The skin overrides body bg, text color, inputs, buttons, table styling per design §5.5, using `:where()` selectors for zero specificity so inline template styles still win where they're explicit.

- [ ] **Step 1: Test scaffold.**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_contains_body_bg_override() {
        let css = generate();
        assert!(css.contains(":where(body)"),
            "skin must use :where(body) for zero specificity");
        assert!(css.contains("--tux-bg") || css.contains("#0c0e12") || css.contains("background"),
            "skin must override body background");
    }

    #[test]
    fn skin_contains_submit_button_styling() {
        let css = generate();
        assert!(
            css.contains("button[type=\"submit\"]") || css.contains("type=submit"),
            "skin must style native submit buttons"
        );
    }

    #[test]
    fn skin_contains_input_styling() {
        let css = generate();
        assert!(css.contains(":where(input"));
        assert!(css.contains(":where(textarea"));
        assert!(css.contains(":where(select"));
    }
}
```

- [ ] **Step 2: Implement.**

```rust
//! tuxlink CSS skin for webview-rendered HTML Forms.
//!
//! Injected by `forms::http_server` at `/skin.css`. Uses `:where()` selectors
//! (zero specificity) so inline template styles win where they're explicit.
//!
//! Design reference: §5.5.

const SKIN_CSS: &str = r#"
/* tuxlink form skin — :where() for zero specificity */
:where(body) {
  background: #0c0e12;
  color: #d6d8dc;
  font-family: -apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  margin: 0;
  padding: 1.5em;
}
:where(input, textarea, select) {
  background: #16181d;
  color: #e6e8ec;
  border: 1px solid #2a2e36;
  border-radius: 4px;
  padding: 0.45em 0.6em;
  font: inherit;
}
:where(input:focus, textarea:focus, select:focus) {
  outline: none;
  border-color: #d97706; /* tuxlink amber */
  box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.18);
}
:where(button) {
  background: #d97706;
  color: #0c0e12;
  border: 1px solid #d97706;
  border-radius: 4px;
  padding: 0.5em 1em;
  font-weight: 600;
  cursor: pointer;
}
:where(button[type="submit"]) { background: #d97706; }
:where(button[type="reset"], button[type="button"]) {
  background: transparent;
  color: #d6d8dc;
  border-color: #2a2e36;
}
:where(table) {
  border-collapse: collapse;
  margin: 1em 0;
  width: 100%;
}
:where(table th, table td) {
  border: 1px solid #2a2e36;
  padding: 0.4em 0.6em;
  text-align: left;
}
:where(table th) { background: #16181d; }
"#;

pub fn generate() -> &'static str { SKIN_CSS }
```

- [ ] **Step 3: Verify + commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib forms::skin 2>&1 | tail -5
git add src-tauri/src/forms/skin.rs src-tauri/src/forms/mod.rs
git commit -m "feat(forms): skin module — tuxlink CSS asset for webview forms

Injected by forms::http_server at /skin.css. :where()-scoped so inline
styles in the WLE template still win where they're explicit.

Refs: bd tuxlink-ytya P1; spec §5.5.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

(Codex adrev on a static-CSS module is low-value; skip the dedicated round and fold it into the http_server adrev in Task 6.)

---

## Task 5: `forms::multipart` — parse urlencoded + multipart preserving repeated names

**Files:**
- New: `src-tauri/src/forms/multipart.rs`

Parses the body of a form-submit POST. Preserves repeated names (WLE forms with table rows, checkbox groups). Distinguishes submitter button (`name="Submit"` button value tells WLE Submit-vs-Cancel). multer 3 owns the multipart heavy lifting; percent-encoding owns urlencoded.

- [ ] **Step 1: Test fixtures.**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    const URLENC_BODY: &str = "Subject=Test&Body=hello&Name=W6ABC&Name=W7DEF&Submit=Submit";

    #[test]
    fn urlencoded_preserves_repeated_names() {
        let parsed = parse_urlencoded(URLENC_BODY).unwrap();
        assert_eq!(parsed.fields.get("Subject"), Some(&vec!["Test".to_string()]));
        // Two `Name` entries must round-trip as a 2-element vec, not coalesce.
        assert_eq!(parsed.fields.get("Name"), Some(&vec!["W6ABC".to_string(), "W7DEF".to_string()]));
        assert_eq!(parsed.submitter, Some("Submit".to_string()));
    }

    #[test]
    fn urlencoded_handles_url_escapes() {
        let parsed = parse_urlencoded("Subject=hello%20world&Body=line1%0Aline2").unwrap();
        assert_eq!(parsed.fields["Subject"][0], "hello world");
        assert_eq!(parsed.fields["Body"][0], "line1\nline2");
    }

    #[tokio::test]
    async fn multipart_preserves_repeated_names_and_submitter() {
        let boundary = "----testboundary";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"Subject\"\r\n\r\nT\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Name\"\r\n\r\nA\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Name\"\r\n\r\nB\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Submit\"\r\n\r\nSend\r\n\
             --{b}--\r\n",
            b = boundary
        );
        let parsed = parse_multipart(boundary, Bytes::from(body)).await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "T");
        assert_eq!(parsed.fields["Name"], vec!["A".to_string(), "B".to_string()]);
        assert_eq!(parsed.submitter, Some("Send".to_string()));
    }
}
```

- [ ] **Step 2: Implement.**

```rust
//! Submit-body parser for HTML Forms — handles urlencoded + multipart while
//! preserving repeated field names (WLE table rows / checkbox groups) and
//! identifying the submitter button (WLE distinguishes Submit vs Cancel via
//! `name="Submit"` button value).
//!
//! Design reference: §5.3 (hardening + Codex adrev).

use bytes::Bytes;
use multer::Multipart;
use percent_encoding::percent_decode_str;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct ParsedBody {
    pub fields: HashMap<String, Vec<String>>,
    /// Submitter button value (e.g. "Submit" / "Cancel") when a button with
    /// `name="Submit"` (or equivalent) was clicked. None for programmatic
    /// submits or non-submit-button submits.
    pub submitter: Option<String>,
}

pub fn parse_urlencoded(body: &str) -> Result<ParsedBody, String> {
    let mut out = ParsedBody::default();
    for pair in body.split('&') {
        if pair.is_empty() { continue; }
        let mut iter = pair.splitn(2, '=');
        let key = iter.next().unwrap_or("");
        let val = iter.next().unwrap_or("");
        let key = percent_decode_str(&key.replace('+', " "))
            .decode_utf8_lossy().into_owned();
        let val = percent_decode_str(&val.replace('+', " "))
            .decode_utf8_lossy().into_owned();
        if key == "Submit" {
            out.submitter = Some(val.clone());
        }
        out.fields.entry(key).or_default().push(val);
    }
    Ok(out)
}

pub async fn parse_multipart(boundary: &str, body: Bytes) -> Result<ParsedBody, String> {
    let mut out = ParsedBody::default();
    let mut mp = Multipart::new(
        futures::stream::once(async move { Ok::<_, std::io::Error>(body) }),
        boundary,
    );
    while let Some(field) = mp.next_field().await.map_err(|e| e.to_string())? {
        let name = field.name().unwrap_or("").to_string();
        let val = field.text().await.map_err(|e| e.to_string())?;
        if name == "Submit" {
            out.submitter = Some(val.clone());
        }
        out.fields.entry(name).or_default().push(val);
    }
    Ok(out)
}
```

NOTE: The multer API expects a `Stream` of `Bytes` results; we wrap the
single-buffer body via `futures::stream::once`. For very large bodies (>1MB)
this should be chunked, but the WLE form-submit bodies are < 100KB in
practice; chunking is an explicit follow-up bd issue.

- [ ] **Step 3: Verify + commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib forms::multipart 2>&1 | tail -10
git add src-tauri/src/forms/multipart.rs src-tauri/src/forms/mod.rs
git commit -m "feat(forms): multipart module — urlencoded + multipart parse

Preserves repeated names (table rows, checkbox groups) and surfaces the
submitter button value so the http_server can distinguish Submit vs
Cancel per WLE convention.

Refs: bd tuxlink-ytya P1; spec §5.3.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 4: Codex adrev.**

Attack-angle prompt focuses on: body-size DOS, encoding-edge-cases (Latin-1 bytes, control chars, no boundary at end), submitter-name aliases (forms may use `submit` lowercase, or different button names per WLE convention).

```bash
cat > /tmp/codex-prompt-multipart.txt <<'EOF'
Adversarial review of forms::multipart module against origin/main.
Run `git diff origin/main -- src-tauri/src/forms/multipart.rs` for the diff.

Attack angles:
1. Body-size DOS: are there any size caps? What happens if a malicious form
   POSTs a 1GB body?
2. Encoding: are Latin-1 bytes that aren't valid UTF-8 (legitimate in WLE
   templates that allow bare ISO-8859-1 in field values) lost or corrupted?
3. Submitter identification: WLE conventionally uses `name="Submit"` but
   templates in the wild may use `name="submit"` (lowercase), `name="Send"`,
   or no name. How does parse handle that?
4. Repeated-name semantics: does the order in `Vec<String>` match the order
   the fields appeared in the body? (Critical for table-row submission where
   row[0].fieldA pairs with row[0].fieldB.)
5. Multipart edge cases: missing boundary, malformed Content-Disposition,
   nested multipart, file attachments (form has type=file inputs).

Read: src-tauri/src/forms/multipart.rs

Output findings as a markdown block at the end.
EOF
cat /tmp/codex-prompt-multipart.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-01-p1-multipart-codex.md
```

Apply P0/P1 findings; file follow-up bd for P2/P3.

---

## Task 6: `forms::http_server` — lazy axum server with per-open token

**Files:**
- New: `src-tauri/src/forms/http_server.rs`
- Delete: `src-tauri/src/forms/spike.rs` (the Task 2 spike) + its IPC command + the React button

The big one. Lifecycle is per-form-open, not application-lifetime. Each open mints a fresh random token; the form URL embeds it; the submit POST validates it. WLE template substitutions baked in. No path traversal. CORS disabled (only the child webview reaches this origin via Tauri capability).

**Routes:**
- `GET /forms/<token>/<id>` — serve the form template with WLE substitutions
- `GET /skin.css` — serve the skin (Task 4)
- `GET /assets/<token>/*path` — serve WLE-template-adjacent assets (CSS, JS, images that the template references); allowlisted per template + path-traversal-rejected
- `POST /submit/<token>` — accept the form submission; parse via Task 5; emit FormPayload to the parent compose window via an in-process channel
- Anything else → 404
- Token mismatch on any path → 403

- [ ] **Step 1: Test scaffold (route-level via `axum::oneshot`).**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot`

    fn fixture_template() -> Template {
        Template {
            id: "TEST_Form".to_string(),
            label: "TEST_Form".to_string(),
            folder: "".to_string(),
            source: TemplateSource::Bundled,
            path: PathBuf::from("/dev/null/never-read-in-test"),
        }
    }

    fn make_server(token: &str, html: &str) -> Server {
        let mut s = Server::for_test();
        s.open_session(token, fixture_template(), html.to_string());
        s
    }

    #[tokio::test]
    async fn token_mismatch_returns_403() {
        let s = make_server("correct-token", "<html></html>");
        let resp = s.router()
            .oneshot(Request::builder()
                .uri("/forms/wrong-token/TEST_Form")
                .body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn form_serves_with_substitutions() {
        let html = r#"<html><body>FORM_SERVER={FormServer} FORM_PORT={FormPort} FORM_FOLDER={FormFolder}</body></html>"#;
        let s = make_server("tok", html);
        // Internally records the bound port for substitution; for test, force
        // a stable substitution context.
        let resp = s.router()
            .oneshot(Request::builder()
                .uri("/forms/tok/TEST_Form")
                .body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("FORM_SERVER=127.0.0.1"));
        assert!(s.contains("FORM_PORT="));
        // Skin link should be injected
        assert!(s.contains("href=\"/skin.css\""));
    }

    #[tokio::test]
    async fn skin_css_serves_with_correct_content_type() {
        let s = make_server("tok", "");
        let resp = s.router()
            .oneshot(Request::builder().uri("/skin.css").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/css; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn submit_with_token_dispatches_to_channel() {
        let s = make_server("tok", "");
        let resp = s.router()
            .oneshot(Request::builder()
                .method("POST")
                .uri("/submit/tok")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from("Subject=Hi&Submit=Submit"))
                .unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Verify the parsed body landed on the in-process channel.
        let parsed = s.recv_submission().await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "Hi");
        assert_eq!(parsed.submitter, Some("Submit".to_string()));
    }

    #[tokio::test]
    async fn assets_path_traversal_returns_404() {
        let s = make_server("tok", "");
        let resp = s.router()
            .oneshot(Request::builder()
                .uri("/assets/tok/../../../../etc/passwd")
                .body(Body::empty()).unwrap())
            .await.unwrap();
        assert!(resp.status() == StatusCode::NOT_FOUND || resp.status() == StatusCode::FORBIDDEN);
    }
}
```

- [ ] **Step 2: Implement the server.**

Core sketch (full module is ~300 lines; expand the skeleton in this step):

```rust
//! Lazy loopback HTTP server for HTML Forms webview path. Lifecycle is
//! per-form-open, NOT application-lifetime: bind `127.0.0.1:0`, serve one
//! form session (template + skin + submit endpoint), tear down on close.
//!
//! Hardening per spec §5.3 + Codex 2026-05-31 review:
//! - per-open random token in the URL + POST target
//! - no path traversal in /assets
//! - no IPC to the child webview (capability scope is HTTP-only)
//! - no listen on 0.0.0.0; loopback ONLY
//!
//! See plan §Task 6 for the routes; see spec §8.2 for the data-flow trace.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use rand::{distributions::Alphanumeric, Rng};

use super::wle_templates::{Template, TemplateSource};
use super::multipart::{parse_urlencoded, parse_multipart, ParsedBody};
use super::skin;

#[derive(Clone)]
struct Session {
    token: String,
    template: Template,
    /// Pre-substituted HTML, ready to serve. (For prod: read from
    /// template.path; for test: in-memory.)
    html: String,
    /// Channel for emitting submitted payloads back to the caller.
    tx: mpsc::UnboundedSender<ParsedBody>,
}

#[derive(Clone)]
pub struct Server {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    port: Arc<Mutex<Option<u16>>>,
}

impl Server {
    pub fn new() -> Self { /* … */ }
    pub fn for_test() -> Self { /* test-only constructor */ }

    /// Open a new form session: mint a token, register the template, return
    /// (token, port, url). Caller is responsible for spawning the listener
    /// on first call.
    pub async fn open_session(
        &self,
        template: Template,
        /* tx returned to caller for await’ing submissions */
    ) -> Result<(String, u16, mpsc::UnboundedReceiver<ParsedBody>), String> {
        // mint token
        let token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        // read template HTML
        let raw = std::fs::read_to_string(&template.path).map_err(|e| e.to_string())?;
        // substitutions deferred to GET handler where port is known
        // …
        // start listener if not already
        // …
        let (tx, rx) = mpsc::unbounded_channel();
        // …
        Ok((token, port, rx))
    }

    pub async fn close_session(&self, token: &str) -> Result<(), String> { /* … */ }

    pub fn router(&self) -> Router { /* … wired in step 3 … */ unimplemented!() }
}
```

Build out the router with the 4 routes from the design. Key implementation
details:

1. **Substitution**: replace `{FormServer}` → `127.0.0.1`, `{FormPort}` →
   the listener's port, `{FormFolder}` → `/forms/<token>/<folder>`. Done
   per-request because port is known only after bind. Use simple
   `replace()` — these are well-defined placeholder strings, NOT a
   templating language.
2. **Skin injection**: insert `<link rel="stylesheet" href="/skin.css">` into
   the `<head>` of the served HTML before sending. If `<head>` is absent
   (malformed template), prepend at the top.
3. **Fallback bridge JS** (`/bridge.js`): minimal JS injected as `<script
   src="/bridge.js">` that provides a `tuxlinkExtract(form)` global. The
   diagnostic / developer-mode fallback submit button (`forms/skin.rs`
   could ship the button HTML; design §5.4 calls it "diagnostic / rescue
   tool"). Provide a stub for P1; iterate in P3 if needed.
4. **Submit handler**: validate token, parse Content-Type, dispatch to
   `parse_urlencoded` or `parse_multipart`, emit on the session's `tx`,
   return a small "Submitted ✓" HTML page so the webview shows it.
5. **Asset serving**: optional in P1 (templates rarely link to adjacent
   assets that aren't already in the snapshot). If skipped, the route
   exists and returns 404; document in the commit.

- [ ] **Step 3: Verify + commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib forms::http_server 2>&1 | tail -10
git add src-tauri/src/forms/http_server.rs src-tauri/src/forms/mod.rs
# Delete the spike (Task 2 cleanup)
git rm src-tauri/src/forms/spike.rs
# Also remove the spike IPC command + the React button (line-level edit)
git add -u src-tauri/src/lib.rs src/App.tsx  # or wherever the spike was wired
git commit -m "feat(forms): http_server module — lazy loopback for HTML Forms

Replaces the Task 2 spike with the real server. Per-open token-scoped
form sessions; tearing down on close. WLE {FormServer}/{FormPort}/
{FormFolder} substitution baked in at serve time. Skin auto-injected.
Submit handler parses urlencoded + multipart bodies (Task 5) and emits
ParsedBody on an in-process channel for the calling compose window.

Hardening per spec §5.3:
- Loopback only (127.0.0.1:0)
- Per-open random 32-char token
- Path traversal in /assets rejected (404/403)
- No CORS (only the child webview reaches this origin via the scoped
  Tauri capability landing in Task 7)
- No IPC exposed to the webview

Refs: bd tuxlink-ytya P1; spec §5.3, §8.2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 4: Codex adrev — minimum one dedicated round (highest-risk surface).**

```bash
cat > /tmp/codex-prompt-http-server.txt <<'EOF'
Adversarial review of forms::http_server against origin/main.
Run `git diff origin/main -- src-tauri/src/forms/http_server.rs` for the diff.

This is the most-attacked surface in P1 — a network listener on the host's
loopback that serves operator-supplied HTML and accepts POSTs that build
outbound radio messages. Attack angles:

1. Token-length / entropy: is 32 alphanumeric chars enough? Are tokens
   constant-time-compared (vs. naive `==`) to prevent timing oracles?
2. Multi-session: what happens if two `open_session` calls race? Do
   tokens collide? Are the listener+router instances shared (correct) or
   spawned-per-session (incorrect)?
3. Path-traversal: enumerate every route handler. Are any paths derived
   from user input ever passed to filesystem APIs without canonicalization?
4. Substitution injection: the {FormServer}/{FormPort}/{FormFolder}
   substitutions are a textual `replace()` — can a custom-form template
   inject `{FormPort}<malicious>` and have the substituted output break
   out of an attribute / inject JavaScript / etc?
5. Skin injection: prepending `<link>` to malformed HTML — what if the
   template has no `<head>` AND a `<script>` block that runs before our
   prepended `<link>`? Does prepending change parse order in a way that
   produces unexpected DOM?
6. Listener leak: if `close_session` is called twice / never / between
   open_session and the receiver awaiting, are tasks left dangling, are
   listeners left bound, are channels leaked?
7. Body-size DOS: is there a `axum::extract::DefaultBodyLimit` on /submit?
   What's the limit, and is it enforced before parse?
8. CORS / Origin: is there a Host header check or an explicit Origin
   allowlist? If `127.0.0.1:<port>` is reachable from another local
   process (any other process on the host), is the token the ONLY
   defense? Is that enough?

Read these files:
- src-tauri/src/forms/http_server.rs
- src-tauri/src/forms/multipart.rs (for the submit-side body parser)
- src-tauri/src/forms/wle_templates.rs (Template surface)

Output findings as a markdown block at the end with P0/P1/P2/P3 severity.
EOF
cat /tmp/codex-prompt-http-server.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-01-p1-http-server-codex.md
wc -l dev/adversarial/2026-06-01-p1-http-server-codex.md  # expect 1500-4000
```

Apply ALL P0 + P1 findings as follow-up commits **before** the PR opens. P2/P3 findings → bd issues filed against `tuxlink-ytya` so the operator can prioritize before merge.

---

## Task 7: `forms-webview.json` Tauri capability

**Files:**
- New: `src-tauri/capabilities/forms-webview.json`
- Update: `src-tauri/tauri.conf.json` (reference the new capability)

Scoped capability for the child webview's label pattern (`compose-form-*`). Loopback HTTP fetch only; **no IPC**.

- [ ] **Step 1: Author the capability.**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "identifier": "forms-webview",
  "description": "HTML Forms child-webview — loopback HTTP only, no IPC.",
  "webviews": ["compose-form-*"],
  "permissions": [
    "core:default"
  ],
  "remote": {
    "urls": ["http://127.0.0.1:*"]
  }
}
```

**IMPORTANT:** `permissions` MUST NOT include anything beyond `core:default`
(the minimal allowlist). No `core:event:*`, no `core:webview:*`, no
`fs:*`, no `shell:*`. Verify by listing the capability's `permissions`
array — it's the entire ACL.

- [ ] **Step 2: Test (Rust integration).**

```rust
// src-tauri/tests/forms_capability_scope.rs
#[test]
fn forms_webview_capability_has_no_ipc_perms() {
    let raw = std::fs::read_to_string("capabilities/forms-webview.json").unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let perms = json["permissions"].as_array().unwrap();
    let perms: Vec<&str> = perms.iter().filter_map(|v| v.as_str()).collect();
    // Hardcoded allowlist: only "core:default" is permitted at v0.10.0.
    assert_eq!(perms, vec!["core:default"]);
    // Verify the remote allowlist is loopback-only.
    let urls = json["remote"]["urls"].as_array().unwrap();
    let urls: Vec<&str> = urls.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(urls, vec!["http://127.0.0.1:*"]);
}
```

- [ ] **Step 3: Verify + commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test forms_capability_scope 2>&1 | tail -5
git add src-tauri/capabilities/forms-webview.json src-tauri/tauri.conf.json src-tauri/tests/forms_capability_scope.rs
git commit -m "feat(forms): forms-webview Tauri capability (loopback HTTP, no IPC)

Spec §5.6 + §10. The child webview's only channel back to tuxlink is the
loopback HTTP server; no IPC, no fs, no shell, no window control. An
assertion-driven test pins the permission list so a future agent can't
accidentally widen the surface.

Refs: bd tuxlink-ytya P1; spec §5.6, §10.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Tauri command surface — `open_webview_form`, `close_webview_form_server`, `forms_list_catalog`

**Files:**
- Update: `src-tauri/src/ui_commands.rs` (or wherever `#[tauri::command]` fns live)
- Update: `src-tauri/src/lib.rs` (register in `invoke_handler`)

Three commands wire the Rust side to React:

| Command | Returns | Purpose |
|---|---|---|
| `forms_list_catalog` | `Vec<Template>` | Feeds `CatalogBrowser` (Task 10) |
| `open_webview_form(form_id)` | `{ url: String, port: u16, token: String }` | Spawns http_server + returns the webview's URL |
| `close_webview_form_server(token)` | `()` | Tears down the http_server when the form closes |

- [x] **Step 1: Implement + test.**

(Tests use the `tauri::test::mock_app` pattern; see existing examples in `ui_commands.rs` if present, else file a small test scaffold.)

Sketch:

```rust
#[tauri::command]
pub async fn forms_list_catalog(app: tauri::AppHandle) -> Result<Vec<Template>, String> {
    let bundle = wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = wle_templates::custom_root_for_app(&app);
    let custom_opt = custom.exists().then_some(custom.as_path());
    wle_templates::list(&bundle, custom_opt).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct OpenFormResult { pub url: String, pub port: u16, pub token: String }

#[tauri::command]
pub async fn open_webview_form(
    form_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, FormsHttpServer>,
) -> Result<OpenFormResult, String> {
    let bundle = wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = wle_templates::custom_root_for_app(&app);
    let custom_opt = custom.exists().then_some(custom.as_path());
    let cat = wle_templates::list(&bundle, custom_opt).map_err(|e| e.to_string())?;
    let template = cat.into_iter().find(|t| t.id == form_id)
        .ok_or_else(|| format!("unknown form: {form_id}"))?;
    let (token, port, mut rx) = state.open_session(template).await?;
    // Stash the receiver: a follow-up tokio task moves submissions into an
    // event the frontend awaits. Per design §8.2 the React side uses a
    // tauri event channel for this — `app.emit_to("compose-form-…", "form-submitted", payload)`.
    let app_clone = app.clone();
    let token_clone = token.clone();
    tokio::spawn(async move {
        while let Some(parsed) = rx.recv().await {
            let _ = app_clone.emit_to(
                format!("compose-form-{token_clone}").as_str(),
                "form-submitted",
                parsed,
            );
        }
    });
    Ok(OpenFormResult {
        url: format!("http://127.0.0.1:{port}/forms/{token}/{form_id}"),
        port,
        token,
    })
}

#[tauri::command]
pub async fn close_webview_form_server(
    token: String,
    state: tauri::State<'_, FormsHttpServer>,
) -> Result<(), String> {
    state.close_session(&token).await
}
```

- [x] **Step 2: Register in `invoke_handler`.**

```rust
.invoke_handler(tauri::generate_handler![
    /* …existing… */
    forms_list_catalog,
    open_webview_form,
    close_webview_form_server,
])
```

- [x] **Step 3: Commit.**

(Codex adrev folded into the http_server round in Task 6; the commands here are thin shims.)

---

## Task 9: React `WebviewFormHost` — child webview embed + fallback chrome

**Files:**
- New: `src/compose/WebviewFormHost.tsx`
- New: `src/compose/WebviewFormHost.css`
- New: `src/compose/WebviewFormHost.test.tsx` (CSS-blind vitest; browser-smoke in PR test plan)

Mounts a child Tauri `WebviewWindow` (label: `compose-form-<token>`) embedded in the compose body region (NOT a separate top-level window — design §8.2 has it inline). Subscribes to the `form-submitted` event on that webview; on receipt, dispatches the parsed FormPayload upward to Compose.tsx and triggers `close_webview_form_server`. Renders a tuxlink-chrome fallback submit button below the webview as a diagnostic (per design §5.4: "diagnostic / rescue tool").

- [x] **Step 1: Test scaffold (CSS-blind).**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { WebviewFormHost } from './WebviewFormHost';

// Mock the Tauri webview APIs so vitest can render the component.
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: { getByLabel: () => null },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('<WebviewFormHost>', () => {
  it('renders a container with the form id and the fallback submit button', () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('webview-form-host')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Submit \(fallback\)/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Cancel/i })).toBeInTheDocument();
  });

  it('calls onCancel when the cancel button is clicked', async () => {
    const onCancel = vi.fn();
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={onCancel} />);
    screen.getByRole('button', { name: /Cancel/i }).click();
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [x] **Step 2: Implement the component skeleton.**

```tsx
import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { listen } from '@tauri-apps/api/event';
import './WebviewFormHost.css';

interface Props {
  formId: string;
  onSubmit: (payload: ParsedBody) => void;
  onCancel: () => void;
}

interface ParsedBody {
  fields: Record<string, string[]>;
  submitter: string | null;
}

interface OpenResult { url: string; port: number; token: string; }

export function WebviewFormHost({ formId, onSubmit, onCancel }: Props) {
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let webview: WebviewWindow | undefined;
    (async () => {
      try {
        const res = await invoke<OpenResult>('open_webview_form', { formId });
        if (cancelled) {
          await invoke('close_webview_form_server', { token: res.token });
          return;
        }
        setToken(res.token);
        const label = `compose-form-${res.token}`;
        webview = new WebviewWindow(label, {
          url: res.url,
          /* embed style/positioning TBD — design §8.2 says inline in compose
             body; the exact mount mechanism is a P1 sub-decision. If the
             cleanest path is a top-level child window with parent=Compose,
             document the choice in the commit. */
        });
        const ul = await listen<ParsedBody>('form-submitted', (e) => onSubmit(e.payload));
        unlisten = ul;
      } catch (e) {
        setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
      if (token) {
        invoke('close_webview_form_server', { token }).catch(() => {/* ignore */});
      }
      webview?.close().catch(() => {/* ignore */});
    };
  }, [formId, onSubmit, token]);

  return (
    <div className="webview-form-host" data-testid="webview-form-host">
      {error && (
        <div className="webview-form-error" role="alert">
          Form failed to open: {error}
        </div>
      )}
      <div className="webview-form-host__chrome">
        <button onClick={onCancel}>Cancel</button>
        <button disabled title="Diagnostic only — use the form's own Submit button">
          Submit (fallback)
        </button>
      </div>
    </div>
  );
}
```

- [x] **Step 3: Verify + commit.**

```bash
pnpm exec vitest run src/compose/WebviewFormHost 2>&1 | tail -10
git add src/compose/WebviewFormHost.tsx src/compose/WebviewFormHost.css src/compose/WebviewFormHost.test.tsx
git commit -m "feat(compose): WebviewFormHost — child webview embed + fallback chrome

Spec §8.2 React side. Subscribes to the form-submitted event from
forms::http_server, dispatches FormPayload upward, and cleans up on
unmount. Fallback Submit button is a diagnostic / rescue tool per
design §5.4 (the canonical submit path is the form's native button →
loopback POST).

Refs: bd tuxlink-ytya P1; spec §8.2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: React `CatalogBrowser` — hierarchical picker (replaces FormPicker)

**Files:**
- New: `src/compose/CatalogBrowser.tsx`
- New: `src/compose/CatalogBrowser.css`
- New: `src/compose/CatalogBrowser.test.tsx`
- Update: `src/compose/Compose.tsx` (replace FormPicker usage in form mode entry)
- Preserved short-term: `src/forms/FormPicker.tsx` (used for the 2 native forms P0 ships; design §7 keep-list)

Hierarchical tree (folders) + flat-search picker for the WLE catalog plus the operator's custom forms. Drives the entry into either native form components (ICS-213, Bulletin) or `WebviewFormHost` (everything else).

- [ ] **Step 1: Test scaffold.**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { CatalogBrowser } from './CatalogBrowser';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'forms_list_catalog') {
      return [
        { id: 'ICS213_Initial', label: 'ICS213_Initial', folder: 'ICS Forms', source: 'Bundled', path: '' },
        { id: 'Bulletin_Initial', label: 'Bulletin_Initial', folder: 'General', source: 'Bundled', path: '' },
        { id: 'ARC213', label: 'ARC213', folder: 'ARC Forms', source: 'Bundled', path: '' },
        { id: 'MyCustom', label: 'MyCustom', folder: '', source: 'Custom', path: '' },
      ];
    }
    return null;
  }),
}));

describe('<CatalogBrowser>', () => {
  it('renders all top-level folders', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText('ICS Forms')).toBeInTheDocument();
    expect(screen.getByText('General')).toBeInTheDocument();
    expect(screen.getByText('ARC Forms')).toBeInTheDocument();
    expect(screen.getByText('Custom')).toBeInTheDocument();
  });

  it('expanding a folder reveals its templates', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByText('ICS Forms'));
    expect(screen.getByText('ICS213_Initial')).toBeInTheDocument();
  });

  it('search filters across folders', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByPlaceholderText(/search forms/i);
    fireEvent.change(input, { target: { value: 'arc' } });
    expect(screen.getByText('ARC213')).toBeInTheDocument();
    expect(screen.queryByText('Bulletin_Initial')).toBeNull();
  });

  it('picking a form fires onPick with the id', async () => {
    const onPick = vi.fn();
    render(<CatalogBrowser onPick={onPick} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByText('ICS Forms'));
    fireEvent.click(screen.getByText('ICS213_Initial'));
    expect(onPick).toHaveBeenCalledWith('ICS213_Initial');
  });
});
```

- [ ] **Step 2: Implement.**

(Skeleton; the full component is ~150 lines. Key invariants: client-side
search filter; folders sorted alphabetically; Custom forms folder always
last; arrow keys for accordion nav optional in P1.)

- [ ] **Step 3: Wire to Compose.tsx.**

In `src/compose/Compose.tsx`, replace the `FormPicker` import + render with
`CatalogBrowser`:

```diff
-import { FormPicker, lookupForm, allForms } from '../forms';
+import { CatalogBrowser } from './CatalogBrowser';
+import { lookupForm } from '../forms';
…
-{formMode.kind === 'pick' && (
-  <FormPicker
-    forms={allForms().map((f) => ({ id: f.id, name: f.name }))}
-    onPick={(id) => setFormMode({ kind: 'form', formId: id, values: {} })}
-    onCancel={() => setFormMode({ kind: 'plain' })}
-  />
-)}
+{formMode.kind === 'pick' && (
+  <CatalogBrowser
+    onPick={(id) => {
+      // Native registry takes precedence; else fall through to webview.
+      const entry = lookupForm(id);
+      if (entry?.Form) {
+        setFormMode({ kind: 'form', formId: id, values: {} });
+      } else {
+        setFormMode({ kind: 'webview-form', formId: id });
+      }
+    }}
+    onCancel={() => setFormMode({ kind: 'plain' })}
+  />
+)}
+{formMode.kind === 'webview-form' && (
+  <WebviewFormHost
+    formId={formMode.formId}
+    onSubmit={(payload) => handleWebviewSubmit(formMode.formId, payload)}
+    onCancel={() => setFormMode({ kind: 'plain' })}
+  />
+)}
```

This requires extending the `FormMode` discriminated union:

```diff
-type FormMode =
-  | { kind: 'plain' }
-  | { kind: 'pick' }
-  | { kind: 'form'; formId: string; values: Record<string, string> };
+type FormMode =
+  | { kind: 'plain' }
+  | { kind: 'pick' }
+  | { kind: 'form'; formId: string; values: Record<string, string> }
+  | { kind: 'webview-form'; formId: string };
```

And the new `handleWebviewSubmit` handler:

```tsx
const handleWebviewSubmit = useCallback(async (formId: string, payload: ParsedBody) => {
  // Convert ParsedBody (multi-value fields) to FormPayload (the shape
  // send_form expects). Single-valued fields → join the [0] entry;
  // multi-valued fields → join with a separator that forms::parse
  // preserves (TBD: design §5.3 implies WLE convention is fine here).
  const fieldValues: Record<string, string> = {};
  for (const [k, vs] of Object.entries(payload.fields)) {
    if (k === 'Submit') continue;
    fieldValues[k] = vs.length === 1 ? vs[0] : vs.join('\n');
  }
  await invoke<string>('send_form', {
    formId,
    fieldValues,
    to: splitAddrs(to),
    cc: splitAddrs(cc),
    sendersCallsign: callsign,
    gridSquare: grid,
  });
  // mirror handleFormSubmit's post-send cleanup
  sentRef.current = true;
  setSendState('success');
  clearDraft(draftId);
  // …
}, [to, cc, callsign, grid, draftId]);
```

- [ ] **Step 4: Verify + commit.**

```bash
pnpm exec vitest run src/compose/ 2>&1 | tail -10
git add src/compose/CatalogBrowser.tsx src/compose/CatalogBrowser.css \
        src/compose/CatalogBrowser.test.tsx src/compose/Compose.tsx
git commit -m "feat(compose): CatalogBrowser + webview-form mode

Hierarchical picker that replaces FormPicker for the form-entry path.
Drives native-form mode for ICS-213/Bulletin (registry-resident with
Form components) and webview-form mode for everything else (delegates
to WebviewFormHost). New 'webview-form' kind in the FormMode union.

Refs: bd tuxlink-ytya P1; spec §7.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Receive-side fallback — render unknown forms via Viewer-mode http_server

**Files:**
- Update: `src-tauri/src/forms/http_server.rs` (add read-only Viewer mode)
- Update: `src/mailbox/MessageView.tsx` (route unknown form_ids to a Viewer webview)

Per design §8.3: when MessageView sees a `form_id` it doesn't have a native View for, mount a webview that loads the WLE `_Viewer.html` template with the parsed FormPayload injected as form values.

- [ ] **Step 1: Extend http_server with a `Viewer` session kind.**

In `forms::http_server`, add an `open_session_viewer` method that:
- Mints a token (same as form mode)
- Reads the `*_Viewer.html` template instead of the form template
- Substitutes WLE placeholders AND injects the FormPayload field values
- Returns the URL
- The /submit endpoint returns 404 (read-only mode)

- [ ] **Step 2: New Tauri command: `open_webview_viewer(form_id, payload)`.**

Symmetric to `open_webview_form` but takes a FormPayload to bind.

- [ ] **Step 3: MessageView changes.**

When `lookupForm(message.formId)` doesn't have a View component (i.e., it's a custom or unknown form), render a `<WebviewFormViewer>` component that calls `open_webview_viewer` and mounts the webview. Falls back to `KeyValueView` if the Viewer template is also missing.

- [ ] **Step 4: Verify + commit.**

```bash
pnpm exec vitest run src/mailbox/ 2>&1 | tail -10
cargo test --manifest-path src-tauri/Cargo.toml --lib forms::http_server 2>&1 | tail -5
git add src-tauri/src/forms/http_server.rs src-tauri/src/ui_commands.rs \
        src/mailbox/MessageView.tsx src/compose/WebviewFormViewer.tsx
git commit -m "feat(forms): receive-side Viewer-mode webview fallback

Spec §8.3. Unknown form_ids fall through from the native View registry
to a Viewer-mode webview that loads the WLE _Viewer.html with the
parsed FormPayload bound. Submit endpoint is 404'd in this mode.

Refs: bd tuxlink-ytya P1; spec §8.3.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Custom-forms directory enumeration

**Files:**
- Update: `src-tauri/src/forms/wle_templates.rs` (custom_root_for_app default)
- No-op for v1: hot-reload + override-via-Settings is P3

Spec §6 P1 says "Custom-forms directory enumeration (`~/.local/share/tuxlink/forms/custom/` default; operator-overridable)". The first half (default location) lands here; the operator-overridable part is P3 alongside the catalog updater.

- [ ] **Step 1: Verify `custom_root_for_app` resolves to the documented path.**

```bash
# Integration test: run the binary briefly with TUXLINK_DATA_DIR pointed at a
# tempdir and verify custom_root_for_app returns <tempdir>/tuxlink/forms/custom
```

- [ ] **Step 2: Document the path in `README.md`.**

```markdown
## Custom HTML Forms

tuxlink reads custom HTML form templates from
`~/.local/share/tuxlink/forms/custom/`. Drop a `*.html` file there; it
appears in the CatalogBrowser as a Custom-folder entry on next launch
(P1) or live (P3, planned).
```

- [ ] **Step 3: Commit.**

```bash
git add README.md
git commit -m "docs: custom HTML forms drop-dir documentation (P1)"
```

---

## Task 13: End-to-end smoke + Codex full-diff adrev + PR open

- [ ] **Step 1: Full vitest + cargo test sweep.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ytya-p1-webview-infra
pnpm exec vitest run 2>&1 | tail -5
cargo --manifest-path src-tauri/Cargo.toml test --lib 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml clippy --all-targets -- -D warnings 2>&1 | tail -5
```

All three must be green before PR.

- [ ] **Step 2: Codex full-diff adrev.**

```bash
cat > /tmp/codex-prompt-p1-full.txt <<'EOF'
Adversarial review of the full P1 diff against origin/main.
Run `git diff origin/main..HEAD` for the diff.

This is the second-pass review after per-module rounds. Focus on
cross-module concerns the per-module reviews can't see:

1. Lifecycle interaction: opening a form mid-compose, then closing the
   compose window before submitting — are sessions / channels leaked?
2. Concurrency: two compose windows simultaneously opening different
   forms — does the http_server demultiplex correctly by token?
3. Capability scope vs runtime invocations: does any new Tauri command
   accept inputs from the form webview origin? (Should not — the
   capability ACL forbids IPC from there.)
4. Receive-side Viewer + send-side form sharing the http_server: when
   one MessageView pane is rendering a Viewer and a Compose window
   opens a form, do they share the same listener or separate? Token
   collision risk?
5. FormPayload round-trip via the webview path: does the data emerge
   on the inbox side identical to native-form-submitted data? (Same
   serialize/parse paths from PR #177 — but verify nothing is dropped
   in the urlencoded → ParsedBody → FormPayload conversion.)

Output P0/P1/P2/P3 severity. Use the bd issue tuxlink-ytya as the
container for follow-ups.
EOF
cat /tmp/codex-prompt-p1-full.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-01-p1-full-diff-codex.md
wc -l dev/adversarial/2026-06-01-p1-full-diff-codex.md  # expect 2000-4000
```

- [ ] **Step 3: Apply P0 + P1 findings as follow-up commits.** File P2/P3 as `bd create` against `tuxlink-ytya` (still in_progress) for operator triage.

- [ ] **Step 4: Push + open PR.**

```bash
git push -u origin bd-tuxlink-ytya/p1-webview-infra
gh pr create --base main --head bd-tuxlink-ytya/p1-webview-infra \
  --title "[<moniker>] feat(forms): P1 webview infrastructure + bundled WLE templates + CatalogBrowser (tuxlink-ytya)" \
  --body-file dev/scratch/2026-06-01-p1-pr-body.md
```

PR body MUST include:

- Summary referencing spec §6 P1
- Per-module list with line-count + test count
- WLE snapshot pin + bundle size (Task 0)
- Codex adrev disposition summary (P0 applied, P1 applied, P2/P3 → bd issues)
- **Browser-smoke walk-through** for the operator:

```
1. Launch `pnpm tauri dev` (or run a built binary).
2. File → New Message → click into the form-picker entry.
3. CatalogBrowser appears with folders: ICS Forms / General / ARC Forms / Custom (if any).
4. Expand ICS Forms → click ICS213_Initial. EXPECTED: native React form opens.
5. Cancel back to picker. Expand ARC Forms → click ARC213. EXPECTED: child
   webview opens inside compose body, showing tuxlink-skinned ARC213 form.
6. Fill ARC213's required fields, click the form's native Submit button.
   EXPECTED: webview dismisses, compose body returns to plain, "Posted to
   Outbox" success state fires.
7. Verify CMS-side: the message arrives with the correct subject + body
   text + RMS_Express_Form_ARC213.xml attachment.
8. Drop a custom HTML file at `~/.local/share/tuxlink/forms/custom/MyTest.html`,
   restart tuxlink. EXPECTED: MyTest appears in the Custom folder.
9. Receive a form-bearing message of an unknown type (use a fixture
   message in dev mode). EXPECTED: MessageView falls through to a
   Viewer-mode webview rendering the form's _Viewer.html.
```

- [ ] **Step 5: Update bd issue with PR ref.**

```bash
bd update tuxlink-ytya --notes "Shipped as PR #<N> on bd-tuxlink-ytya/p1-webview-infra. Awaiting operator browser smoke per PR body. Per-module Codex adrev rounds completed: wle_templates, multipart, http_server, full-diff. P0/P1 findings applied inline. P2/P3 findings filed: <bd-issue-ids>."
# Leave open (in_progress) until operator merges. Browser-smoke gate per
# feedback_browser_smoke_before_ship.
```

---

## Out-of-scope follow-ups (carried to P2/P3)

- **Hot-reload custom-forms directory** (P3): inotify watch + catalog refresh on file change.
- **Operator-override custom-forms directory** (P3): Settings UI + config-file plumbing.
- **Viewer-mode UI polish** (P3): currently a bare webview; could get a
  tuxlink-chrome wrapper with "Reply to form" action.
- **Submit-time validation against FormDef** (P1, gated on Codex finding):
  the design's §10 step 6 calls for `FormDef` validation before
  incorporating the submission into `OutboundMessage`. The Task 5 parser
  produces raw ParsedBody; the http_server submit handler should
  cross-check fields against the form's declared field set before
  emitting on the channel. If Codex flags this as P0/P1, fix in P1; else
  carry to a follow-up bd.
- **Body-size cap** on /submit (P1, gated on Codex finding): per the
  http_server adrev attack angle #7 — needs a `DefaultBodyLimit` on the
  router; default to 1MB and document.

---

## Acceptance criteria

- [ ] Bundled WLE Standard Forms snapshot present in binary, ≤ 20 MB
- [ ] `forms::wle_templates::list` returns bundled + custom forms with
      custom-overrides-bundled semantics; 4+ unit tests green
- [ ] `forms::skin::generate()` returns a CSS string covering body,
      inputs, buttons, table styling; 3+ unit tests green
- [ ] `forms::multipart` parses urlencoded + multipart, preserves repeated
      names + submitter; 3+ unit tests green
- [ ] `forms::http_server` lazy-binds 127.0.0.1:0, mints per-open
      32-char tokens, substitutes WLE placeholders, injects skin link,
      parses submits, emits on channel; 6+ route-level unit tests green
- [ ] `forms-webview.json` capability has ONLY `core:default` permission
      and `http://127.0.0.1:*` remote; assertion-driven test pins the ACL
- [ ] Three new Tauri commands (`forms_list_catalog`, `open_webview_form`,
      `close_webview_form_server`) registered + tested
- [ ] React `WebviewFormHost` renders + mounts a child webview; CSS-blind
      vitest covers the wrapper chrome
- [ ] React `CatalogBrowser` replaces FormPicker; native-vs-webview
      routing intact; 4+ vitest cases green
- [ ] Receive-side Viewer fallback path lands; unknown form_ids no longer
      render as KeyValueView-only
- [ ] Custom-forms drop-dir documented in README
- [ ] Codex adrev: per-module rounds + full-diff round, P0+P1 findings
      applied, P2+P3 filed as bd issues
- [ ] `pnpm vitest run` and `cargo test --lib` and `cargo clippy -D warnings`
      all green
- [ ] PR opened with operator browser-smoke checklist in body
- [ ] bd `tuxlink-ytya` notes carry the PR URL and the Codex disposition
      summary; status stays in_progress until operator merge

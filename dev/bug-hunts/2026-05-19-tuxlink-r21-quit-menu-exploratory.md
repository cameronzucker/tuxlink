# Bug Hunt Report — tuxlink-r21 File → Quit (exploratory)

Date: 2026-05-19
Hunter: willow-cypress-heron (exploratory mode)
Scope: `bd-tuxlink-r21/fix-quit-native` worktree at /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-r21-fix-quit-native/
Method: depth-first reading of tauri-2.11.0 + muda-0.19.1 + tauri-runtime-wry-2.11.0 source in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. All claims about Tauri/muda runtime behavior are **verified from source, not inferred from training data or docs prose alone.**

## Scope

Files explored:
- `src-tauri/src/menu.rs` (HEAD = 4a0b19a, attempt 2 — PredefinedMenuItem::quit)
- `src-tauri/src/menu.rs` (40a7f1d, attempt 1 — custom MenuItemBuilder + `app.exit(0)`)
- `src-tauri/src/lib.rs` (setup callback, unchanged between attempts)
- `src-tauri/tests/menu_test.rs` (event-id manifest test)
- `src-tauri/Cargo.toml` + `tauri.conf.json`
- Tauri 2.11.0 source: `src/menu/predefined.rs`, `src/menu/menu.rs`, `src/app.rs`, `src/manager/menu.rs`
- muda 0.19.1 source: `src/items/predefined.rs`, `src/platform_impl/gtk/mod.rs`
- tauri-runtime-wry 2.11.0 source: `src/lib.rs` (request_exit + RunEvent::ExitRequested)

## Findings

### Bug A — `PredefinedMenuItem::quit` is silently dropped from GTK menus (attempt 2)

**Location:** `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/muda-0.19.1/src/platform_impl/gtk/mod.rs:30-50` and `:803-826`
**Severity:** critical (matches the observed "Quit absent from File menu" symptom exactly)
**Verified from source.**

**Evidence chain:**

1. `tauri::menu::PredefinedMenuItem::quit` (`tauri-2.11.0/src/menu/predefined.rs:318-334`) wraps `muda::PredefinedMenuItem::quit`. Its rustdoc literally says `**Linux:** Unsupported.` (line 317). This is not aspirational documentation; it reflects implementation behavior below.
2. `muda::PredefinedMenuItem::quit` (`muda-0.19.1/src/items/predefined.rs:147-149`) constructs a `PredefinedMenuItemType::Quit` enum variant. Construction always succeeds — no Linux check at this layer. Hence `build_menu` returns `Ok(_)` and the lib build is clean.
3. **The drop happens at insertion time, in the GTK platform impl.** `muda-0.19.1/src/platform_impl/gtk/mod.rs:30-50` defines the `is_item_supported!` macro:

   ```rust
   macro_rules! is_item_supported {
       ($item:tt) => {{
           let child = $item.child();
           let child_ = child.borrow();
           let supported = if let Some(predefined_item_type) = &child_.predefined_item_type {
               matches!(
                   predefined_item_type,
                   PredefinedMenuItemType::Separator
                       | PredefinedMenuItemType::Copy
                       | PredefinedMenuItemType::Cut
                       | PredefinedMenuItemType::Paste
                       | PredefinedMenuItemType::SelectAll
                       | PredefinedMenuItemType::About(_)
               )
           } else { true };
           ...
       }};
   }
   ```

   `PredefinedMenuItemType::Quit` is **not in the allowlist.** Submenu's `add_menu_item` (line 803-838) wraps its GTK-widget creation in `if is_item_supported!(item) { ... }`. Quit fails the check, **no `gtk::MenuItem` is created, nothing is appended to the GTK menu, no `.show()` is called.** The unsupported item IS still pushed into `self.children` (line 829) so the in-memory menu model knows about it — that's why the operator's hand-written `menu_event_ids()` manifest test sees it conceptually, but the rendered GTK menubar omits it.
4. Even if rendering succeeded, the accelerator for Quit is **macOS-only.** `muda-0.19.1/src/items/predefined.rs:338-340`:
   ```rust
   #[cfg(target_os = "macos")]
   PredefinedMenuItemType::Quit => Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyQ)),
   _ => None,
   ```
   So on Linux, `PredefinedMenuItem::quit(app, Some("Quit"))` registers no Ctrl+Q binding (matches "Ctrl+Q accelerator does nothing").
5. **Tauri's own canonical default menu acknowledges this.** `tauri-2.11.0/src/menu/menu.rs:205-221` cfg-gates the entire File submenu (which contains `PredefinedMenuItem::quit`) to NOT compile on Linux/BSD:
   ```rust
   #[cfg(not(any(target_os = "linux", target_os = "dragonfly", ...)))]
   &Submenu::with_items(app_handle, "File", true, &[
       &PredefinedMenuItem::close_window(app_handle, None)?,
       #[cfg(not(target_os = "macos"))]
       &PredefinedMenuItem::quit(app_handle, None)?,
   ])?,
   ```
   Tauri's own design treats `PredefinedMenuItem::quit` as a non-functional no-op on Linux and omits the entire File menu rather than render an empty one.

**Impact:** Attempt 2 is structurally a dead end on Linux. The Quit entry will never render, regardless of label, regardless of build flags, regardless of accelerator string. There is no feature flag, no AboutMetadata predecessor, no setup-order tweak that fixes this; it's a hard allowlist in the GTK platform impl.

### Question B — attempt 1's empty-window startup symptom: cannot determine from static reading

**Inquiry result:** The reported symptom ("window opens empty, no menu, no webview content, pnpm ELIFECYCLE, no Rust panic, no Tauri error") **cannot be explained by the code change in 40a7f1d.** I exhausted every plausible mechanism in the source. Stating "cannot determine from static reading; needs runtime instrumentation" per the report contract.

**What I verified that rules out the obvious hypotheses:**

1. **`on_menu_event` does not fire synchronously during setup.** `tauri-2.11.0/src/manager/menu.rs:97-103` shows registration is a simple `Vec::push`. The drain site is `tauri-2.11.0/src/app.rs:2580-2595`, which only fires for `EventLoopMessage::MenuEvent` events flowing from the muda event handler (`app.rs:2342-2344`). The muda gtk impl only emits MenuEvents in `connect_activate` callbacks (`muda-0.19.1/src/platform_impl/gtk/mod.rs:1116-1118, 1290-1292, 1355-1357`) — i.e., user click handlers. No synthetic events are emitted at menu-construction time. There is no path by which attempt 1's `app_for_handler.exit(0)` could have run during setup.
2. **`app.exit(0)` is fully async and routed through the event-loop proxy.** `tauri-2.11.0/src/app.rs:574-580` → `runtime_handle.request_exit(0)` → `tauri-runtime-wry-2.11.0/src/lib.rs:2751-2758` sends `Message::RequestExit(0)` over the proxy. The receive side at `tauri-runtime-wry-2.11.0/src/lib.rs:4361-4374` only fires `RunEvent::ExitRequested` and sets `control_flow = ControlFlow::Exit`. It does NOT bypass the webview load sequence, does NOT touch GTK menubar state, does NOT corrupt the window.
3. **Tauri's official `on_menu_event` doc example IS exactly attempt 1's pattern.** `tauri-2.11.0/src/app.rs:1980-1989` (Builder::on_menu_event docs):
   ```rust
   tauri::Builder::default()
     .on_menu_event(|app, event| {
        if event.id() == "quit" {
          app.exit(0);
        }
     });
   ```
   The only difference vs attempt 1 is `on_menu_event` was wired against the `AppHandle` from inside `setup` (`wire_menu_events(app.handle())`) rather than as a Builder method, but both routes terminate at the same `MenuManager::on_menu_event` push (`tauri-2.11.0/src/app.rs:812` and `:1996` both call `self.manager.menu.on_menu_event(handler)` / push to the same Vec).
4. **No type/borrow/compile issue.** `app.clone()` on `AppHandle<R>` is cheap (clones the inner Arc), the closure captures `app_for_handler` and `_app` is the listener's own arg, both `emit` and `exit` are callable on `AppHandle<R>`. The reported `cargo test --test menu_test` pass + `cargo build --lib` clean is consistent with this.

**Speculation, marked as such (NOT verified from source — needs runtime instrumentation):**

- The "ELIFECYCLE" with no Rust panic strongly suggests the binary started, then exited cleanly with non-zero code, OR exited via `std::process::exit` somewhere — a panic would print to stderr. The `pnpm tauri dev` wrapper's ELIFECYCLE is just pnpm reporting the child process returned non-zero.
- One unverified hypothesis: there's a known GTK/Wayland class of bug where `gtk::Application` quit signals fire during early init under certain compositor states. None of the muda/tauri code I read references that, but I did not exhaustively read the GTK accelerator-installation paths.
- Another unverified hypothesis: operator's terminal output may have been truncated or buffered, and a panic message did fire but wasn't seen. This is testable: re-run the exact attempt-1 build with `RUST_BACKTRACE=1 RUST_LOG=trace tauri dev 2>&1 | tee /tmp/attempt1.log`.

**Recommendation for runtime instrumentation if attempt 1 is revisited:** add a `dbg!("wire_menu_events called")` immediately before `app.on_menu_event(...)` and a `dbg!("menu event:", &id)` at the top of the closure. If the first prints and the second doesn't, the closure is registered but never invoked — symptom is somewhere outside the menu path. If neither prints, setup itself is bailing before `wire_menu_events`.

### Bug C — `menu_event_ids()` is testing the wrong contract

**Location:** `src-tauri/src/menu.rs:25-48` + `src-tauri/tests/menu_test.rs:14-40`
**Severity:** significant (this is a designed-in blind spot, not a typo)

**Evidence:** The module's own doc-comment (menu.rs:14-17) acknowledges the test is intentionally a manifest of event IDs, not an assertion of platform-rendered menu items. The current bug (Bug A) is invisible to this test by construction: `PredefinedMenuItem::quit` doesn't emit an event ID via the `menu_event_ids()` manifest (and the comment at menu.rs:28-32 explains why it's deliberately omitted in attempt 2). But the actual failure mode — "the menu item doesn't render on Linux" — is also invisible. The test passes for two different reasons in attempts 1 vs 2, and neither catches "the user sees no Quit entry."

**Impact:** Both attempts shipped with green tests. Operator-side manual smoke is the only path that catches the actual failure. This is acknowledged in the plan's "Manual verification tax" section per the test-file header. **This finding is documented here for completeness; it is consistent with the existing design decision, not a bug per se.** Adding a real GTK-rendering smoke test is out of scope for r21.

## Recommendation (canonical Linux Quit pattern)

The correct pattern, verified from `tauri-2.11.0/src/app.rs:1980-1989` (Builder::on_menu_event official example) and from the surface-area analysis above, is **attempt 1's design**: a custom `MenuItemBuilder::with_id("menu:file:quit", "Quit", "CmdOrCtrl+Q")` plus a branch in the `on_menu_event` closure that calls `app.exit(0)` when the id matches. This works because:

- `MenuItemBuilder::build` produces a normal (non-predefined) GTK menu item, which has no allowlist gate and renders unconditionally.
- The `CmdOrCtrl+Q` accelerator is honored on GTK (the `register_accel!` macro at `muda-0.19.1/src/platform_impl/gtk/mod.rs:997+` registers normal-item accelerators unconditionally).
- The `on_menu_event` handler is the documented integration point for `app.exit`.

**The blocker is reproducing attempt 1's empty-window symptom under instrumentation to confirm it was a transient build/install artifact (not a structural bug).** Reading every plausible code path produced no mechanism by which 40a7f1d should fail at startup. Reasonable next move: re-checkout 40a7f1d, `cargo clean`, `pnpm tauri dev 2>&1 | tee /tmp/attempt1.log`, and inspect — the failure is very likely a stale build artifact or a webview-load race not caused by the menu change.

Optional belt-and-suspenders: hook `tauri::WindowEvent::CloseRequested` on the main window in addition to the menu handler. This catches window-close-button quits and also provides the future intercept point for "discard unsaved draft?" dialogs (Task 14).

## Design concerns

- **Three layers of "supported" filtering, only one of which has a compile-time check.** `tauri::menu::PredefinedMenuItem::quit` returns `tauri::Result<Self>` but the GTK filter at `muda-0.19.1/src/platform_impl/gtk/mod.rs:30-50` is a silent runtime drop. There is no `Err` returned, no log line, no diagnostic. Operators only learn the item didn't render by looking at the menubar. This is a structural foot-gun in the muda API surface; nothing tuxlink can do about it beyond avoiding `PredefinedMenuItem` for Linux-targeted code paths. Worth a pitfalls.md entry.
- **Tauri docs vs source disagree about what's "Unsupported."** The rustdoc says "Unsupported"; the construction succeeds and the Result is Ok. A developer reasonably reading the type signature concludes "construction would error if unsupported on this platform" — but it doesn't. The doc would be more accurate as "Silently no-op on Linux."

## Testing-pitfalls.md update

Recommend adding an entry: "MENU-1 — Tauri `PredefinedMenuItem::*` items can be silently dropped on Linux without error. The `cargo test` + `cargo build` gates pass; the item simply doesn't render at runtime. For any menu work, the only adequate verification is operator-driven GTK smoke (pnpm tauri dev → click through every submenu). Static menu-id manifest tests verify the in-memory model only, NOT the rendered GTK widget tree."

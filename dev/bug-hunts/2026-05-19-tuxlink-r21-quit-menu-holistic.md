# Bug Hunt Report — tuxlink-r21 Quit menu, holistic pass

Branch: `bd-tuxlink-r21/fix-quit-native` at HEAD `4a0b19a` (Attempt 2); prior `40a7f1d` (Attempt 1) examined via `git show`.
Hunter: willow-cypress-heron, holistic read-everything-then-reason.

## Scope

Read in full:
- Worktree: `src-tauri/src/menu.rs` (HEAD), prior `40a7f1d:src-tauri/src/menu.rs`, `src-tauri/src/lib.rs`, `src-tauri/tests/menu_test.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src/App.tsx`.
- Cargo registry: `tauri-2.11.0/src/menu/predefined.rs`, `tauri-2.11.0/src/menu/menu.rs`, `tauri-2.11.0/src/manager/menu.rs`, `tauri-2.11.0/src/app.rs` (setup + on_menu_event + set_menu + AppHandle::exit), `muda-0.19.1/src/items/predefined.rs`, `muda-0.19.1/src/platform_impl/gtk/mod.rs`.

All "Tauri does X" / "muda does X" claims below are **verified from source** with file:line citations. Items labelled "inferred" are explicitly flagged.

## Bugs

### B1 — `PredefinedMenuItem::quit` is documented Linux-unsupported AND silently dropped from rendered GTK menus
**Location:** `src-tauri/src/menu.rs:54` (call site); evidence in muda+Tauri sources.
**Severity:** critical (Attempt 2 root cause).
**Evidence:**

1. Tauri 2.11.0 explicitly documents Linux as unsupported for `PredefinedMenuItem::quit`:
   ```
   /// ## Platform-specific:
   ///
   /// - **Linux:** Unsupported.
   pub fn quit<M: Manager<R>>(manager: &M, text: Option<&str>) -> crate::Result<Self> { ... }
   ```
   `tauri-2.11.0/src/menu/predefined.rs:313-334`.

2. The Tauri team's OWN default menu (`Menu::default`) gates `PredefinedMenuItem::quit` behind `#[cfg(not(any(target_os = "linux", ...)))]` — they don't even attempt it on Linux:
   ```
   #[cfg(not(any(target_os = "linux", target_os = "dragonfly", ...)))]
   &Submenu::with_items(app_handle, "File", true, &[
     &PredefinedMenuItem::close_window(app_handle, None)?,
     #[cfg(not(target_os = "macos"))]
     &PredefinedMenuItem::quit(app_handle, None)?,
   ])?,
   ```
   `tauri-2.11.0/src/menu/menu.rs:205-221`.

3. The concrete mechanism by which the item disappears is in muda's GTK backend. `muda-0.19.1/src/platform_impl/gtk/mod.rs:30-50` defines `is_item_supported!` which returns `true` only for these `PredefinedMenuItemType`s: `Separator | Copy | Cut | Paste | SelectAll | About(_)`. Everything else (Quit, CloseWindow, Minimize, Maximize, Hide, Fullscreen, etc.) is "not supported".

4. `Menu::add_menu_item` at `gtk/mod.rs:98-127` gates GTK widget creation on `is_item_supported!`:
   ```
   pub fn add_menu_item(&mut self, item: &dyn crate::IsMenuItem, op: AddOp) -> crate::Result<()> {
       if is_item_supported!(item) {
           for (menu_id, menu_bar) in &self.gtk_menubars {
               let gtk_item = item.make_gtk_menu_item(...)?;
               menu_bar.append(&gtk_item);
               gtk_item.show();
           }
           ...
       }
       match op {
           AddOp::Append => self.children.push(item.child()),  // still tracked in children
           ...
       }
       Ok(())
   }
   ```
   The unsupported item is **silently appended to `self.children` but no GTK widget is ever created** — and `Ok(())` is returned. No error. No log. The submenu variant `Submenu::add_menu_item` at `gtk/mod.rs:803-838` uses the same pattern. `add_menu_item_with_id` and `add_menu_item_to_context_menu` use the `return_if_item_not_supported!` early-return macro (`gtk/mod.rs:52-58, 129-144`).

5. In `make_gtk_menu_item` for predefined items (`gtk/mod.rs:1166-1239`), the match arm is `_ => unreachable!()`. So if `is_item_supported!` ever lied and Quit slipped through, the program would panic on the `unreachable!` — but `is_item_supported!` gates this correctly, so Quit silently no-ops at render time without panicking.

**Impact:** On the worktree's runtime (Pi 5, Linux, webkit2gtk-4.1), the File menu builds successfully (`Result::Ok`), the menu bar shows all 7 categories, but the Quit entry never gets a GTK widget — exactly the operator's observation: "Quit entry is ABSENT… Ctrl+Q does nothing." `cargo test` doesn't catch this because the test surface is the pure `menu_event_ids()` Vec, which never instantiates a GTK widget. `cargo build` doesn't catch it because the call type-checks and returns `Ok`.

### B2 — Attempt 2's `menu_event_ids()` manifest is a faithful contract that doesn't reflect Quit's runtime brokenness
**Location:** `src-tauri/src/menu.rs:25-48`, `src-tauri/tests/menu_test.rs:14-40`.
**Severity:** minor (correctness of the design comment, not the code).
**Evidence:** The comment at `menu.rs:28-32` claims Quit is "bound to PredefinedMenuItem::quit, which Tauri handles natively (it never fires on_menu_event)." Strictly true that it doesn't fire on_menu_event — but the comment implies the binding works on this platform. Per B1, it doesn't. The comment should explicitly say "on Linux/GTK this is a no-op; Quit does not appear in the menu bar."

**Impact:** A reader who trusts the comment thinks Quit ships when it doesn't. Combined with the manual-verification policy already documented in `menu_test.rs:8-10`, this is a comment defect, not a behavior bug — but it actively misleads the next maintainer.

## Design Concerns

### Attempt 1's empty-window mystery — cannot be attributed to the menu code from static reading
Re-examined the 40a7f1d implementation:
```rust
app.on_menu_event(move |_app, event| {
    let id = event.id().as_ref().to_string();
    let _ = app_for_handler.emit("menu", &id);
    if id == "menu:file:quit" {
        app_for_handler.exit(0);
    }
});
```
`AppHandle::on_menu_event` at `tauri-2.11.0/src/manager/menu.rs:97-103` and `tauri-2.11.0/src/app.rs:808-812` simply pushes the boxed closure onto `global_event_listeners: Mutex<Vec<...>>`. **No synthetic events are dispatched during setup.** The dispatch site is `tauri-2.11.0/src/app.rs:2577-2595`:
```
RuntimeRunEvent::UserEvent(t) => match t {
    EventLoopMessage::MenuEvent(ref e) => {
        for listener in &*app_handle.manager.menu.global_event_listeners.lock().unwrap() {
            listener(app_handle, e.clone());
        }
        ...
    }
}
```
The listener fires only when the muda event loop forwards a real menu click (`app.rs:2342-2343`). There is no code path from `app.set_menu(menu)?` or `wire_menu_events(app.handle())` to a synthetic `menu:file:quit` event in setup. `AppHandle::exit` at `app.rs:573-580` requires being called.

**Therefore: the static reading of Attempt 1 contains no mechanism for the observed empty-window-and-exit-at-startup symptom.** Static reading is exhausted; the symptom must originate elsewhere. The most likely culprit, **inferred not verified**, is the `pnpm tauri dev` race: pnpm spawns `pnpm dev` (Vite at localhost:1420) in parallel with `cargo run`; if Vite isn't ready when webkit2gtk navigates, the webview is empty. "ELIFECYCLE Command failed" is pnpm's report that the child process (the Rust binary) exited non-zero — consistent with a normal-exit code carried by the user pressing window-close or by an unrelated startup error. The absence of any Rust panic / Tauri error in stderr is consistent with this. **Cannot determine from static reading; needs runtime instrumentation (RUST_LOG=tauri=debug, plus a Vite-ready barrier) to confirm.**

If Attempt 1's empty-window symptom IS reproducible on a clean rebuild from `40a7f1d`, the bisect target is environmental (Vite race, Wayland/GDK_BACKEND, webkit2gtk-4.1 quirks), not the Quit code.

### Canonical Tauri 2 Linux Quit pattern
From the source survey, the canonical pattern for cross-platform Quit on Linux in Tauri 2.x is:

1. Use `MenuItemBuilder::with_id("menu:file:quit", "Quit").accelerator("CmdOrCtrl+Q").build(app)?` (a custom MenuItem, NOT PredefinedMenuItem). This renders correctly on GTK because custom MenuItems go through `gtk/mod.rs::create_gtk_text_menu_item` (not the predefined gate).
2. In `on_menu_event`, match the id and call `app.exit(0)` — `AppHandle::exit` (`app.rs:574-580`) goes through `runtime_handle.request_exit`, fires `RunEvent::ExitRequested` then `RunEvent::Exit`, and falls back to `cleanup_before_exit + std::process::exit` if the runtime request fails. This is the right primitive.
3. For future intercept work (Task 14 unsaved-draft dialog), hook `tauri::WindowEvent::CloseRequested` on the main window AND/OR add an `on_event` handler for `RunEvent::ExitRequested` that calls `api.prevent_exit()`. The menu-event handler can still emit a `menu` IPC event for the React frontend to react to (status bar update, prompt, etc.), then conditionally call `app.exit(0)`.

**Recommended fix:** revert to the Attempt 1 SHAPE (custom MenuItem + on_menu_event branch that calls `app.exit(0)`), but verify the empty-window symptom on a clean rebuild. If it reproduces, that's an independent bug to bisect — likely the Vite race, not the Quit code. The PredefinedMenuItem path is a dead end on Linux per the Tauri team's own default-menu code (`menu.rs:205-211` skips it).

### Provenance summary (verified vs. inferred)
- **Verified from source:** B1 mechanism (Tauri docstring + Tauri default-menu cfg + muda is_item_supported + muda add_menu_item). Canonical fix shape (custom MenuItemBuilder works on GTK; on_menu_event dispatch site; AppHandle::exit semantics).
- **Inferred (not verified):** Attempt 1's empty-window symptom being the Vite-ready race. Static reading rules out the menu code as cause; the actual root cause for that single observation needs runtime instrumentation.

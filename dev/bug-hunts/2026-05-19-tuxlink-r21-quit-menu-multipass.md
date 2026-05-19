# Bug Hunt Report — tuxlink-r21 Quit menu, multipass

Branch: `bd-tuxlink-r21/fix-quit-native` (worktree at `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-r21-fix-quit-native/`)
Tauri 2.11.0, muda 0.19.1, tauri-runtime-wry 2.11.0, tao 0.35.2
Target: Linux/GTK (Pi 5, Wayland → GTK3, webkit2gtk-4.1)

## Scope

Multipass analysis of the Quit-menu wiring across two attempts.

- `src-tauri/src/menu.rs` (HEAD = attempt 2, commit `4a0b19a`)
- `src-tauri/src/menu.rs` @ `40a7f1d` (attempt 1)
- `src-tauri/src/lib.rs`
- `src-tauri/src/main.rs`
- `src-tauri/Cargo.toml` / `tauri.conf.json`
- Tauri 2.11.0 source: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tauri-2.11.0/src/{app.rs, menu/predefined.rs, manager/menu.rs}`
- muda 0.19.1 source: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/muda-0.19.1/src/{items/predefined.rs, platform_impl/gtk/mod.rs}`
- tauri-runtime-wry 2.11.0: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tauri-runtime-wry-2.11.0/src/lib.rs`

Passes 1, 2, 3, 5 performed. Pass 4 (concurrency) had nothing to chew on — `on_menu_event` just pushes a closure into a `Mutex<Vec<...>>` (manager/menu.rs:97-103).

---

## Bug 1 — `PredefinedMenuItem::quit` is a no-op on Linux (verified from source)

**Location:** `src-tauri/src/menu.rs:54` — `.item(&PredefinedMenuItem::quit(app, Some("Quit"))?)`

**Severity:** critical — this is Attempt 2's exact symptom.

**Evidence (verified from muda source, not inferred):**

1. muda's public doc says so explicitly: `muda-0.19.1/src/items/predefined.rs:144-147`:
   ```rust
   /// Quit app menu item
   /// ## Platform-specific:
   /// - **Linux:** Unsupported.
   pub fn quit(text: Option<&str>) -> PredefinedMenuItem { ... }
   ```
   Tauri's wrapper at `tauri-2.11.0/src/menu/predefined.rs:313-317` copies that doc comment verbatim.

2. The mechanism that drops it: muda's GTK platform impl uses an `is_item_supported!` macro at `muda-0.19.1/src/platform_impl/gtk/mod.rs:30-50`. It whitelists exactly `Separator | Copy | Cut | Paste | SelectAll | About(_)` — **`Quit` is not in the whitelist**.

3. The gate is checked in `Menu::add_menu_item` at `gtk/mod.rs:98-108`:
   ```rust
   pub fn add_menu_item(&mut self, item: &dyn crate::IsMenuItem, op: AddOp) -> crate::Result<()> {
       if is_item_supported!(item) {
           // ... build & insert gtk widget ...
       }
       // else: silently returns Ok(()) — no widget, no error
   }
   ```
   Submenu `.item(...)` calls follow the same gating path. No `Result::Err` is produced; the item is simply omitted from the gtk MenuBar.

4. Defense in depth — even if it slipped past the gate, the per-item builder at `gtk/mod.rs:1131-1239` matches `Separator | Copy/Cut/Paste/SelectAll | About(_)` and falls through to `_ => unreachable!()` (line 1238) for everything else, including `Quit`. The gate is what saves us from the `unreachable!()`.

**Impact:** On Linux, the "Quit" entry never appears in the File submenu, no accelerator is registered with `gtk::AccelGroup`, so Ctrl+Q does nothing. The native menu bar still renders the other 6 categories (and File → New) because those use ordinary `MenuItemBuilder`. This precisely matches the operator's smoke for attempt 2.

**Found in:** Pass 1 — Contract Violations (the function's name promises a Quit item; on Linux it silently does nothing).

---

## Bug 2 — Attempt 1's "exit-at-startup" cannot be reproduced from static reading

**Location:** `src-tauri/src/menu.rs` @ `40a7f1d:124-131` — the `wire_menu_events` closure that called `app_for_handler.exit(0)` inside the on_menu_event handler.

**Severity:** unable to verify from static reading.

**Evidence (what the source actually does):**

1. `App::on_menu_event` (`tauri-2.11.0/src/app.rs:806-813`) and `Manager::on_menu_event` (`tauri-2.11.0/src/manager/menu.rs:97-103`) only push the closure onto `Mutex<Vec<...>>`. **No synchronous invocation**, no synthetic event during setup. The closure cannot fire from `app.set_menu(menu)?` followed by `crate::menu::wire_menu_events(app.handle())` (lib.rs:24-26).

2. `AppHandle::exit(0)` (`tauri-2.11.0/src/app.rs:573-580`) calls `runtime_handle.request_exit(0)`. In tauri-runtime-wry (`lib.rs:2751-2758`), that sends a `Message::RequestExit(code)` UserEvent to the event loop. The event loop handles it at `lib.rs:4361-4374` by firing `RunEvent::ExitRequested` and setting `ControlFlow::Exit`. This is async w.r.t. setup — it cannot kill the binary before setup returns.

3. There is a panic site at `tauri-runtime-wry-2.11.0/src/lib.rs:3347`:
   ```rust
   Message::RequestExit(_code) => panic!("cannot handle RequestExit on the main thread"),
   ```
   But this is in `handle_user_message`, which is dispatched only for messages NOT sent via `proxy.send_event` — i.e., for the `run_on_main_thread` path. `app.exit(0)` always goes through `proxy.send_event` (line 2756), reaching the `Event::UserEvent` branch at line 4361, not the panic branch. So this panic site does NOT fire from a normal `app.exit(0)`.

4. Tauri's own published example at `tauri-2.11.0/src/app.rs:1985-1987` literally uses the attempt-1 pattern:
   ```rust
   .on_menu_event(|app, event| {
      if event.id() == "quit" {
        app.exit(0);
      }
   });
   ```
   That is the canonical idiom.

**What I cannot determine from static reading:** the "window opens empty + ELIFECYCLE Command failed" startup symptom for attempt 1. The static code path for attempt 1 is correct. Static analysis predicts: window opens normally, native menu renders all 7 categories including Quit, clicking Quit emits the `menu:file:quit` event and calls `app.exit(0)` which gracefully exits the binary (which would in turn make `pnpm tauri dev` print ELIFECYCLE — that's `pnpm`'s normal reaction to a child binary that exits cleanly mid-dev-session).

**Hypothesis (NOT verified — needs runtime instrumentation):** the operator may have observed `pnpm tauri dev` reacting to a CLEAN exit (after they clicked Quit) and misattributed the empty-window screenshot. `pnpm` exits the parent process when the child terminates; if the dev server was the one with the visible window and that died with the binary, the operator would see the window vanish (not "open empty"). Alternative: a frontend HMR disconnect/cache state from the prior attempt's run masquerading as a backend failure. The Rust code in attempt 1 is correct against Tauri 2.11.0; I cannot find a mechanism in Tauri/muda/wry/tao source that would cause attempt 1 to exit at startup.

**Recommended diagnostic for next reproduction:**
- Run `RUST_LOG=tauri=debug,wry=debug pnpm tauri dev 2>&1 | tee /tmp/tuxlink-quit.log`
- Look for `failed to exit:` (logged at `tauri-2.11.0/src/app.rs:576` if `request_exit` fails)
- Verify whether the binary actually crashes or whether `pnpm`'s ELIFECYCLE is the only signal.
- A `panic = "abort"` profile flag elsewhere could explain a missing panic message, but a search of Cargo.toml shows no such setting.

**Found in:** Pass 3 — Failure Mode Reasoning (traced the full exit pipeline).

---

## Bug 3 — Quit accelerator string platform-portability

**Location:** `src-tauri/src/menu.rs` @ `40a7f1d:49` — `.accelerator("CmdOrCtrl+Q")`.

**Severity:** minor (not the cause of either failure; flagging for completeness).

**Evidence:** GTK accelerator parsing (`muda-0.19.1/src/platform_impl/gtk/accelerator.rs:28-56`) handles ASCII keys and explicit codes correctly. `CmdOrCtrl+Q` parses fine on Linux. Not a bug — included only to head off a wrong-track diagnosis.

---

## Canonical cross-platform Quit pattern on Linux/Tauri 2 (Question C answer)

**Verified from Tauri source, not inferred.** The canonical pattern, as published in Tauri's own example at `tauri-2.11.0/src/app.rs:1980-1989`, is:

1. Build the Quit item as a normal `MenuItemBuilder::with_id("quit", "Quit").accelerator("CmdOrCtrl+Q")` — NOT `PredefinedMenuItem::quit` (which is Linux-broken per Bug 1).
2. In the `on_menu_event` handler, dispatch on the id and call `app.exit(0)`.

**For Task 14's "discard unsaved draft?" intercept:** there are two clean options.
- **Option A:** Keep Quit as a custom MenuItem (the attempt-1 shape). Show the confirm dialog from inside the on_menu_event handler before deciding whether to call `app.exit(0)`. This is the simplest path and matches the example.
- **Option B:** Subscribe to `RunEvent::ExitRequested` via `app.run(|app_handle, event| { ... })` and use the `ExitRequestedEventAction::Prevent` reply to veto. Source: `tauri-runtime-wry-2.11.0/src/lib.rs:4361-4373` and `tauri-2.11.0/src/app.rs:4323-4327`. Hooking `WindowEvent::CloseRequested` (as the current menu.rs doc comment suggests) catches WM close-button + Alt-F4 but does NOT fire from a menu-driven `app.exit(0)`; for menu-Quit intercept you need on_menu_event OR ExitRequested.

The doc comment in `menu.rs:30-32` and `:118-123` directing future work to `WindowEvent::CloseRequested` is technically incomplete — that hook catches the window-close path but not the explicit-quit path. Worth correcting when the Quit item is restored.

---

## Recommended fix for tuxlink-r21

Revert to attempt 1's shape with one small clarifying tweak — that pattern is what Tauri's own docs prescribe:

```rust
// menu.rs:50-55 area
let file = SubmenuBuilder::new(app, "File")
    .item(&MenuItemBuilder::with_id("menu:file:new", "New Message").accelerator("CmdOrCtrl+N").build(app)?)
    .separator()
    .item(&MenuItemBuilder::with_id("menu:file:quit", "Quit").accelerator("CmdOrCtrl+Q").build(app)?)
    .build()?;

// wire_menu_events: keep the explicit exit branch, restore "menu:file:quit" in menu_event_ids()
pub fn wire_menu_events<R: Runtime>(app: &AppHandle<R>) {
    let app_for_handler = app.clone();
    app.on_menu_event(move |_app, event| {
        let id = event.id().as_ref().to_string();
        let _ = app_for_handler.emit("menu", &id);
        if id == "menu:file:quit" {
            app_for_handler.exit(0);
        }
    });
}
```

Before declaring it fixed: run `pnpm tauri dev` and BOTH (a) confirm the Quit item visibly renders, (b) click it and observe the binary exits with code 0 (no panic, no orphaned process). If attempt 1 truly broke startup on this Pi, the diagnostic logging recipe in Bug 2 will localize it — but the static source evidence is that attempt 1's code is correct.

---

## Design Concerns

- **`menu_event_ids()` is a hand-written manifest that lives separately from `build_menu()`.** The test (`tests/menu_test.rs`) verifies the manifest contains the expected IDs but does NOT verify the manifest matches what `build_menu` actually emits. A future drift (adding a menu item but forgetting the manifest) passes both `cargo test` and `cargo build`. This is exactly the failure mode that made attempt 2 ship — the test was green because the manifest was internally consistent, but the actual GTK menu was missing the Quit widget. Consider either generating the manifest from `build_menu` (introspecting `Menu<R>` is awkward, hence the current shape) or adding a runtime test that walks the menu tree (would need a real `App` instance — costly).
- **Static unit tests are insufficient for platform-conditional menu code.** Two changes in a row passed `cargo test` and `cargo build --lib` and were still wrong at runtime. The project's existing pitfall PITFALL-BROWSER-SMOKE (`feedback_browser_smoke_before_ship.md`) already covers this for CSS; the parallel rule for native menu work should be: any change to `menu.rs` requires `pnpm tauri dev` + visual confirmation before commit.
- **`PredefinedMenuItem` items in general are a Linux footgun.** Eight of muda's predefined items are documented `Linux: Unsupported` (Undo, Redo, Minimize, Maximize, Fullscreen, Hide, HideOthers, ShowAll, CloseWindow, Quit, Services, BringAllToFront). For a Linux-native app (tuxlink), `PredefinedMenuItem` is essentially limited to Separator + Copy/Cut/Paste/SelectAll + About. Prefer custom MenuItems with explicit accelerators throughout.

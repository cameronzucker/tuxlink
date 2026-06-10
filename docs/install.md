# Install and first-run guide

This guide covers installing tuxlink and completing the onboarding wizard on a
first run. For build-time toolchain setup, see [development.md](development.md).

## Install options

### Option 1: prebuilt AppImage (forthcoming)

A prebuilt AppImage will arrive on the
[Releases page](https://github.com/cameronzucker/tuxlink/releases) once the CI
release pipeline lands (tracked separately from this guide). When available:

1. Download `tuxlink_<version>_amd64.AppImage` from the Releases page.
2. Make it executable: `chmod +x tuxlink_*.AppImage`
3. Run it: `./tuxlink_*.AppImage`

No build toolchain required. **The AppImage cannot bundle the keyring daemon.** See [Runtime prerequisite](#runtime-prerequisite-secret-service-keyring) below.

### Option 2: build from source

See [development.md](development.md) for the full toolchain table, system package
commands, and build invocation. The short version:

```bash
git clone https://github.com/cameronzucker/tuxlink.git
cd tuxlink/src-tauri
cargo build --release
```

## Runtime prerequisite: secret-service keyring

On Linux, tuxlink stores the Winlink CMS password in the OS keyring via the
secret-service D-Bus interface. **A compatible keyring daemon must be running
before tuxlink launches.** This is the most common first-run blocker.

See [development.md: Runtime prerequisites for end-users](development.md#runtime-prerequisites-for-end-users)
for the full list of which desktops require action and which do not. The short summary:

- **GNOME / GNOME-derived desktops** (Ubuntu, Fedora, Debian GNOME): `gnome-keyring-daemon`
  ships with the desktop and is usually already running. No action needed.
- **KDE Plasma**: `kwalletd` ships with the desktop and exposes the secret-service
  interface to non-KDE apps. Usually no action needed.
- **Minimal / tiling WM installs** (i3, sway, Openbox, and similar): install and
  start a secret-service provider. Easiest path:
  ```bash
  sudo apt install gnome-keyring libsecret-1-0
  ```
  Then ensure `gnome-keyring-daemon --daemonize --components=secrets` runs in your
  session (or add it to your session startup). See
  [development.md](development.md#runtime-prerequisites-for-end-users) for details.
- **macOS**: native Keychain Services. Always available; no action needed.
- **Windows**: native CredentialManager. Always available; no action needed.

If tuxlink's wizard reports "keyring backend unavailable" or "secret-service not
running," resolve this before continuing. See [Troubleshooting](#troubleshooting)
below.

## First run: the onboarding wizard

On first launch, tuxlink opens the onboarding wizard. The wizard has three steps.

### Step 1: Choose a connection mode

The first screen asks: **"Will this installation connect to the Winlink CMS?"**

- **Yes, connect to the Winlink CMS:** the default for most operators. Uses the
  internet-backed CMS for authentication. Enter a callsign and CMS password next.
- **No, this is an offline / radio-only deployment:** for ARES drills, EOC
  tabletops, Winlink Hybrid Network operators, and lab work. Tuxlink attempts no
  CMS connection. The offline path skips credentials entirely.

### Step 2 (CMS path): Winlink account credentials

Enter the callsign and CMS password associated with the
[Winlink account](https://www.winlink.org/user/register). No account yet? The
wizard provides a registration link.

- **Callsign:** required. Must match the Winlink account callsign.
- **CMS password:** required. Tuxlink stores it in the OS keyring immediately on
  submit and never writes it to a config file on disk.
- **Grid locator:** optional (4-character Maidenhead, e.g. `EM75`). Powers
  position-proximity features.
- **MBO address:** optional; auto-fills to `<callsign>@winlink.org`. Change only
  to override the default mail-box operator address.

Two submit paths:
- **Continue:** saves credentials and proceeds to the test-send step.
- **Save credentials and skip verification:** saves credentials and goes directly
  to the inbox, bypassing the test send.

### Step 2 (offline path): Station identity

For offline deployments, tuxlink asks for an optional station identifier and grid
locator. Both fields are optional; tuxlink runs fully offline with no identity
configured. Identity can be set later via **Tools → Settings**.

### Step 3: Verify CMS credentials (optional test send)

The test-send step sends a brief message to `SERVICE@winlink.org` and waits for an
autoresponder reply, verifying that credentials are correct and CMS connectivity
works end-to-end.

**Transport:** tuxlink connects to the CMS via TLS on port 8773 by default. If the
network blocks port 8773, change the transport in **Settings → Connection** and
retry.

- **Send test:** initiates the test send. A live session log displays the CMS
  session as it progresses.
- **Skip:** bypasses the test send and goes directly to the inbox. Tuxlink has
  already saved the credentials; you can retry the test send later from
  **Session → Test send**.

If the test send fails, the wizard displays likely causes (no internet connection,
firewall blocking port 8773, CMS temporarily busy, captive portal intercepting
traffic) and offers **Retry**, **Edit credentials**, or **Go to inbox** without
re-entering the wizard.

## Using tuxlink

After the wizard completes, tuxlink opens the main mailbox window.

### Mailbox

The **folder sidebar** on the left lists Inbox, Outbox, Sent, Drafts, and Deleted.
Select a folder to populate the message list. Select a message to open it in the
reading pane on the right. The Inbox badge displays the unread count; the Sent
badge displays the total count.

### Compose and send

Open **File → New Message** (or the compose button in the dashboard ribbon) to
open a compose window. Fill in the To, Subject, and body fields and send. The
reading pane's reply actions handle replies.

### Session log

The session log strip below the reading pane shows a live, human-readable
projection of the CMS session as it progresses: connecting, uploading outbox,
downloading inbox, and disconnecting. Toggle it via **View → Session log**.

### Sync / CMS connection

Tuxlink initiates a CMS session when the operator sends a message or triggers a
sync manually. The status bar at the bottom displays the current connection state
and unread count.

**After the first CMS sync**, new messages appear in the Inbox. The Inbox remains
empty until the first sync completes; this behavior is expected, not an error.

## Uninstall and user-data cleanup

Package removal keeps user data by default. This matches Linux desktop package
expectations and avoids root-level maintainer scripts deleting data from the
wrong home directory or OS keyring. After `apt remove tuxlink`, `dnf remove
tuxlink`, or AppImage deletion, messages, contacts, settings, station catalogs,
logs, webview cache, and keyring credentials may still exist in the user's XDG
profile.

Run the cleanup flow from the same user account before uninstalling, or after
reinstalling if you already removed the package. In the desktop app, open
**Help → Uninstall Cleanup…** to preview the current user's cleanup targets and
run the same cleanup modes from the UI. From a terminal, use:

```bash
tuxlink cleanup --dry-run
tuxlink cleanup
```

Choices:

1. Keep user data. This is the normal uninstall behavior.
2. Remove transient state only: webview cache/storage, map tile cache, logs,
   window state, and stale pid files. Mailbox messages, contacts, stations,
   drafts, settings, and credentials are kept.
3. Remove all Tuxlink operator data: config, mailbox/messages, contacts,
   stations, drafts/search databases, logs/cache/state, user-local launcher
   leftovers, and known Tuxlink keyring entries.

Full cleanup deletes keyring entries for the configured callsigns it can
discover, the fixed listener station-password entry, and peer-password entries
for callsigns found in Tuxlink listener/favorites files. Secret Service does not
let Tuxlink enumerate every account under the `tuxlink` or legacy `tuxlink-pat`
services, so inspect those services manually with a keyring manager if you used
old builds with callsigns no longer present on disk.

## Troubleshooting

### "Keyring backend unavailable" or "secret-service not running"

The OS keyring daemon is not running. See the [Runtime prerequisite](#runtime-prerequisite-secret-service-keyring)
section above. On minimal / tiling WM installs, install `gnome-keyring` and start
the daemon in your session startup, then re-run tuxlink.

### "Keyring is locked"

The keyring daemon is running but the keyring is locked. Unlock it via your
desktop's keyring tool (Seahorse on GNOME, KWalletManager on KDE), then click
Retry in the wizard.

### CMS connection fails at port 8773

A firewall may block outbound TCP port 8773, or a captive portal may intercept
it. Options:

1. Switch to Telnet (port 8772) in **Settings → Connection**. Note that Telnet
   is unencrypted; use only on trusted networks.
2. Connect from a network that allows port 8773 (most home broadband and cellular).
3. If behind a corporate firewall, check whether your IT policy allows port 8773.

### Inbox is empty after wizard completes

The first CMS sync populates the Inbox. Trigger a sync by sending a message, or
use **Session → Sync** once that menu item is available. If no messages arrive
after a successful sync, the Winlink account may have no pending mail; send a
message to `SERVICE@winlink.org` and sync again to confirm the round-trip works.

### Build errors

See [development.md](development.md) for common build-from-source issues, including
the Tauri 2.x system dependency list.

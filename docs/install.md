# Install and first-run guide

This guide covers installing tuxlink and completing the onboarding wizard on a
first run. For build-time toolchain setup, see [development.md](development.md).

## Install options

### Option 1 — prebuilt AppImage (forthcoming)

A prebuilt AppImage will be available from the
[Releases page](https://github.com/cameronzucker/tuxlink/releases) once the CI
release pipeline lands (tracked separately from this guide). When available:

1. Download `tuxlink_<version>_amd64.AppImage` from the Releases page.
2. Make it executable: `chmod +x tuxlink_*.AppImage`
3. Run it: `./tuxlink_*.AppImage`

No build toolchain required. The AppImage bundles the Pat sidecar binary. **The
AppImage cannot bundle the keyring daemon** — see [Runtime prerequisite](#runtime-prerequisite-secret-service-keyring)
below.

### Option 2 — build from source

See [development.md](development.md) for the full toolchain table, system package
commands, submodule setup, and build invocation. The short version:

```bash
git clone --recurse-submodules https://github.com/cameronzucker/tuxlink.git
cd tuxlink/src-tauri
cargo build --release
```

The release build compiles tuxlink and builds the Pat sidecar from
`external/tuxlink-pat/` automatically.

## Runtime prerequisite: secret-service keyring

On Linux, tuxlink stores the Winlink CMS password in the OS keyring via the
secret-service D-Bus interface. **A compatible keyring daemon must be running
before tuxlink launches.** This is the most common first-run blocker.

See [development.md — Runtime prerequisites for end-users](development.md#runtime-prerequisites-for-end-users)
for the full list of what desktops need action and what do not. The short summary:

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
- **macOS**: native Keychain Services — always available, no action needed.
- **Windows**: native CredentialManager — always available, no action needed.

If tuxlink's wizard shows "keyring backend unavailable" or "secret-service not
running," resolve this before continuing. See [Troubleshooting](#troubleshooting)
below.

## First run: the onboarding wizard

On first launch, tuxlink opens the onboarding wizard. The wizard has three steps.

### Step 1 — Choose a connection mode

The first screen asks: **"Will this installation connect to the Winlink CMS?"**

- **Yes, connect to the Winlink CMS** — the default for most operators. Uses the
  internet-backed CMS for authentication. Enter a callsign and CMS password next.
- **No, this is an offline / radio-only deployment** — for ARES drills, EOC
  tabletops, Winlink Hybrid Network operators, and lab work. No CMS connection
  attempts are made. The offline path skips credentials entirely.

### Step 2 (CMS path) — Winlink account credentials

Enter the callsign and CMS password associated with the
[Winlink account](https://www.winlink.org/user/register). No account yet? The
wizard provides a registration link.

- **Callsign** — required. Must match the Winlink account callsign.
- **CMS password** — required. Stored in the OS keyring immediately on submit;
  never written to a config file on disk.
- **Grid locator** — optional (4-character Maidenhead, e.g. `EM75`). Used for
  position-proximity features.
- **MBO address** — optional; auto-fills to `<callsign>@winlink.org`. Change only
  to override the default mail-box operator address.

Two submit paths:
- **Continue** — saves credentials and proceeds to the test-send step.
- **Save credentials and skip verification** — saves credentials and goes directly
  to the inbox, bypassing the test send.

### Step 2 (offline path) — Station identity

For offline deployments, tuxlink asks for an optional station identifier and grid
locator. Both fields are optional — tuxlink works fully offline with no identity
configured. Identity can be set later via **Tools → Settings**.

### Step 3 — Verify CMS credentials (optional test send)

The test-send step sends a brief message to `SERVICE@winlink.org` and waits for an
autoresponder reply, verifying that credentials are correct and CMS connectivity
works end-to-end.

**Transport:** tuxlink connects to the CMS via TLS on port 8773 by default. If the
network blocks port 8773, change the transport in **Settings → Connection** and
retry.

- **Send test** — initiates the test send. A live session log shows the CMS session
  as it progresses.
- **Skip** — bypasses the test send and goes directly to the inbox. Credentials are
  already saved; the test send can be retried later from **Session → Test send**.

If the test send fails, the wizard shows likely causes (no internet connection,
firewall blocking port 8773, CMS temporarily busy, captive portal intercepting
traffic) and offers **Retry**, **Edit credentials**, or **Go to inbox** without
re-entering the wizard.

## Using tuxlink

After the wizard completes, tuxlink opens the main mailbox window.

### Mailbox

The **folder sidebar** on the left lists Inbox, Outbox, Sent, Drafts, and Deleted.
Select a folder to populate the message list. Select a message to open it in the
reading pane on the right. The Inbox badge shows the unread count; the Sent badge
shows the total count.

### Compose and send

Open **File → New Message** (or the compose button in the dashboard ribbon) to open
a compose window. Fill in the To, Subject, and body fields and send. Replies are
available from the reading pane's reply actions.

### Session log

The session log strip below the reading pane shows a live, human-readable
projection of the CMS session as it progresses — connecting, uploading outbox,
downloading inbox, and disconnecting. Toggle it via **View → Session log**.

### Sync / CMS connection

Tuxlink initiates a CMS session when a message is sent or when the operator
triggers a sync manually. The status bar at the bottom shows the current connection
state and unread count.

**After the first CMS sync**, new messages appear in the Inbox. The Inbox is empty
until the first sync completes — this is expected behavior, not an error.

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

Outbound TCP port 8773 may be blocked by a firewall or intercepted by a captive
portal. Options:

1. Switch to Telnet (port 8772) in **Settings → Connection** — note that Telnet is
   unencrypted; use only on trusted networks.
2. Connect from a network that allows port 8773 (most home broadband and cellular).
3. If behind a corporate firewall, check whether your IT policy allows port 8773.

### Inbox is empty after wizard completes

The Inbox is populated on the first CMS sync. Trigger a sync by sending a message,
or use **Session → Sync** once that menu item is available. If no messages arrive
after a successful sync, the Winlink account may have no pending mail — send a
message to `SERVICE@winlink.org` and sync again to confirm the round-trip works.

### Build errors

See [development.md](development.md) for common build-from-source issues, including
the Go toolchain requirement (needed for the Pat sidecar build in release mode) and
the Tauri 2.x system dependency list.

#!/usr/bin/env bash
# uninstall-desktop-entry.sh — remove the userland-installed Tuxlink desktop
# integration that scripts/install-desktop-entry.sh creates. Symmetric to the
# installer: same dual-app_id coverage, same paths, opposite operation.
#
# What it does:
#   1. Removes ~/.local/share/applications/{tuxlink,com.tuxlink.app}.desktop
#   2. Removes ~/.local/share/icons/hicolor/<size>/apps/{tuxlink,com.tuxlink.app}.png
#      under each size the installer creates (32x32, 128x128, 256x256, 512x512)
#   3. Refreshes the desktop database + icon cache (best-effort)
#
# Idempotent: re-runs are safe — missing files are silently skipped (with a
# note logged). Does NOT touch system paths (/usr/share/...) — those are the
# .deb's responsibility and are cleaned by `apt remove tuxlink`.
#
# Run from the repo root (or any worktree):
#   bash scripts/uninstall-desktop-entry.sh
#
# Why this script exists separately from `apt remove`:
#   The .deb installs to /usr/share/applications/ and /usr/share/icons/hicolor/.
#   `apt remove` cleans those system paths correctly. BUT the workaround script
#   install-desktop-entry.sh also drops user-local copies to ~/.local/share/...
#   to work around a Tauri 2.x runtime app_id mismatch (see install-desktop-
#   entry.sh header for the full context). Those user-local files survive
#   `apt remove` because they were never owned by the .deb. This uninstaller
#   removes them.
#
# Tuxlink-md17 closes the asymmetry: install had no companion uninstall, so
# operators removing the reference build saw lingering menu entries after
# `apt remove`.

set -euo pipefail

# Bail gracefully on non-Linux (the installer is Linux-only too).
case "$(uname -s)" in
  Linux) ;;
  *)
    printf '✗ uninstall-desktop-entry.sh is Linux-only — current OS is %s.\n' \
      "$(uname -s)" >&2
    exit 1
    ;;
esac

ICON_DIR="${HOME}/.local/share/icons/hicolor"
APPS_DIR="${HOME}/.local/share/applications"

# Dual app_id coverage: same set the installer uses. If a future installer
# adds a third app_id, this set must grow to match.
APP_IDS=("tuxlink" "com.tuxlink.app")

# Same size set the installer uses.
ICON_SIZES=("32x32" "128x128" "256x256" "512x512")

removed_count=0
skipped_count=0

# --- Step 1: remove .desktop entries -----------------------------------------
for app_id in "${APP_IDS[@]}"; do
  dest_desktop="${APPS_DIR}/${app_id}.desktop"
  if [ -f "${dest_desktop}" ]; then
    rm -f "${dest_desktop}"
    printf '✓ removed %s\n' "${dest_desktop}"
    removed_count=$((removed_count + 1))
  else
    printf '· %s — not present, skipping\n' "${dest_desktop}"
    skipped_count=$((skipped_count + 1))
  fi
done

# --- Step 2: remove icons under each hicolor size ----------------------------
for size in "${ICON_SIZES[@]}"; do
  dest_dir="${ICON_DIR}/${size}/apps"
  for app_id in "${APP_IDS[@]}"; do
    dest="${dest_dir}/${app_id}.png"
    if [ -f "${dest}" ]; then
      rm -f "${dest}"
      printf '✓ removed %s\n' "${dest}"
      removed_count=$((removed_count + 1))
    else
      printf '· %s — not present, skipping\n' "${dest}"
      skipped_count=$((skipped_count + 1))
    fi
  done
done

# --- Step 3: refresh caches (best-effort) -------------------------------------
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APPS_DIR}" 2>/dev/null \
    && printf '✓ ran update-desktop-database %s\n' "${APPS_DIR}" \
    || printf '! update-desktop-database failed (non-fatal; some DEs auto-refresh)\n' >&2
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  # See installer's matching block for why redirecting stderr is correct here.
  gtk-update-icon-cache -f -t "${ICON_DIR}" >/dev/null 2>&1 \
    && printf '✓ refreshed gtk icon cache for %s\n' "${ICON_DIR}" \
    || printf '! gtk-update-icon-cache skipped (no index.theme; normal)\n'
fi

printf '\n'
printf 'Tuxlink desktop entry removed for user %s.\n' "${USER}"
printf 'Removed %d file(s); %d were already absent.\n' "${removed_count}" "${skipped_count}"
printf '\n'

if [ "${removed_count}" -eq 0 ]; then
  printf 'No user-local files were present. The installer may not have been run, or\n'
  printf 'a previous uninstall already cleaned everything up.\n'
  printf '\n'
fi

printf 'If the menu entry persists after this:\n'
printf '  - On GNOME/KDE: log out and back in (the menu cache is refreshed at session start)\n'
printf '  - On labwc + wf-panel-pi (Raspberry Pi): restart wf-panel-pi to refresh:\n'
printf '      pkill wf-panel-pi   # lwrespawn restarts it automatically\n'
printf '  - Also check whether the .deb is still installed:\n'
printf '      dpkg -l | grep -i tuxlink\n'
printf '    If yes, `sudo apt remove tuxlink` cleans the system-level install.\n'

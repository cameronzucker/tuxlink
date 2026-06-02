#!/usr/bin/env bash
# install-desktop-entry.sh — register Tuxlink with the Linux desktop so the
# taskbar / dock / app-switcher show the Tuxlink icon instead of the default
# missing-app icon.
#
# What it does:
#   1. Copies src-tauri/icons/* into ~/.local/share/icons/hicolor/<size>/apps/
#      under the canonical name com.tuxlink.app.png (the reverse-DNS identifier
#      from tauri.conf.json, which becomes the Wayland app_id at runtime).
#   2. Writes ~/.local/share/applications/com.tuxlink.app.desktop with the
#      appropriate Categories + StartupWMClass=com.tuxlink.app so the WM can
#      map the running window back to this .desktop entry.
#   3. Refreshes the desktop database + icon cache (best-effort).
#
# Idempotent: re-runs are safe and just re-install the latest files. Adds
# nothing to system paths (/usr/share/...) — userland install only.
#
# Run from the repo root (or any worktree):
#   bash scripts/install-desktop-entry.sh
#
# Tuxlink-mj7i: closes the "taskbar shows default Tauri icon" regression.

set -euo pipefail

# Bail gracefully on non-Linux (macOS / Windows have their own bundle paths).
case "$(uname -s)" in
  Linux) ;;
  *)
    printf '✗ install-desktop-entry.sh is Linux-only — current OS is %s.\n' \
      "$(uname -s)" >&2
    printf '  On macOS, Tauri uses the .app bundle (no install step needed).\n' >&2
    printf '  On Windows, Tauri registers via the MSI installer.\n' >&2
    exit 1
    ;;
esac

# Resolve REPO_ROOT from the script's own location (not cwd / git toplevel),
# so the script works regardless of where the operator invoked it from. The
# script lives at scripts/install-desktop-entry.sh in the worktree it's
# resolving for — that's the correct source of truth.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ICONS_SRC="${REPO_ROOT}/src-tauri/icons"
APP_ID="com.tuxlink.app"   # tauri.conf.json identifier → GApplication ID → Wayland app_id
ICON_DIR="${HOME}/.local/share/icons/hicolor"
APPS_DIR="${HOME}/.local/share/applications"
DESKTOP_FILE="${APPS_DIR}/${APP_ID}.desktop"

if [ ! -d "${ICONS_SRC}" ]; then
  printf '✗ %s not found — are you in the tuxlink repo?\n' "${ICONS_SRC}" >&2
  exit 1
fi

# --- Step 1: install icons into hicolor theme ---------------------------------
# Map each source size to its canonical hicolor directory. Tauri's icon set
# carries 32x32, 128x128, 128x128@2x (= 256x256), plus a high-res icon.png
# (used here as the 512x512 master). PNG remains PNG; no rasterization needed.

declare -A ICON_MAP=(
  ["32x32"]="${ICONS_SRC}/32x32.png"
  ["128x128"]="${ICONS_SRC}/128x128.png"
  ["256x256"]="${ICONS_SRC}/128x128@2x.png"
  ["512x512"]="${ICONS_SRC}/icon.png"
)

for size in "${!ICON_MAP[@]}"; do
  src="${ICON_MAP[${size}]}"
  if [ ! -f "${src}" ]; then
    printf '! source icon missing — skipping %s: %s\n' "${size}" "${src}" >&2
    continue
  fi
  dest_dir="${ICON_DIR}/${size}/apps"
  dest="${dest_dir}/${APP_ID}.png"
  mkdir -p "${dest_dir}"
  cp -f "${src}" "${dest}"
  printf '✓ installed %s → %s\n' "${size}" "${dest}"
done

# --- Step 2: install .desktop file -------------------------------------------
# The canonical .desktop file lives at scripts/${APP_ID}.desktop — same source
# of truth that tauri.conf.json's bundle.linux.deb.files ships into a
# production .deb at /usr/share/applications/. Both paths install the SAME
# file so dev and production agree byte-for-byte.
SRC_DESKTOP="${REPO_ROOT}/scripts/${APP_ID}.desktop"
if [ ! -f "${SRC_DESKTOP}" ]; then
  printf '✗ source .desktop file missing: %s\n' "${SRC_DESKTOP}" >&2
  exit 1
fi
mkdir -p "${APPS_DIR}"
cp -f "${SRC_DESKTOP}" "${DESKTOP_FILE}"
chmod 644 "${DESKTOP_FILE}"
printf '✓ installed %s → %s\n' "${SRC_DESKTOP}" "${DESKTOP_FILE}"

# --- Step 3: refresh caches (best-effort) -------------------------------------
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APPS_DIR}" 2>/dev/null \
    && printf '✓ ran update-desktop-database %s\n' "${APPS_DIR}" \
    || printf '! update-desktop-database failed (non-fatal; some DEs auto-refresh)\n' >&2
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  # gtk-update-icon-cache requires an index.theme in the cache directory;
  # ~/.local/share/icons/hicolor usually does NOT carry one (system theme
  # supplies it). -f to force; -t to ignore mtime; redirect stderr because
  # absent index.theme is a normal noise.
  gtk-update-icon-cache -f -t "${ICON_DIR}" >/dev/null 2>&1 \
    && printf '✓ refreshed gtk icon cache for %s\n' "${ICON_DIR}" \
    || printf '! gtk-update-icon-cache skipped (no index.theme; this is normal — many DEs read icons live without a cache)\n'
fi

printf '\n'
printf 'Tuxlink desktop entry installed for user %s.\n' "${USER}"
printf '\n'
printf 'Next steps:\n'
printf '  1. Kill the running tauri dev session (Ctrl+C).\n'
printf '  2. Relaunch with: pnpm tauri dev\n'
printf '  3. The taskbar / dock should now show the Tuxlink icon.\n'
printf '\n'
printf 'If the icon still shows the default after a relaunch:\n'
printf '  - On GNOME/KDE: log out and back in (the icon cache is refreshed at session start)\n'
printf '  - On labwc: nothing further needed; new windows pick up the icon immediately\n'
printf '  - On Sway: same as labwc\n'
printf '  - Verify .desktop file: cat %s\n' "${DESKTOP_FILE}"
printf '  - Verify icon installed: ls %s/*/apps/%s.png\n' "${ICON_DIR}" "${APP_ID}"

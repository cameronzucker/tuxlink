#!/usr/bin/env bash
# install-desktop-entry.sh — register Tuxlink with the Linux desktop so the
# taskbar / dock / app-switcher show the Tuxlink icon instead of the default
# missing-app icon.
#
# What it does:
#   1. Copies src-tauri/icons/* into ~/.local/share/icons/hicolor/<size>/apps/
#      under TWO names: tuxlink.png (binary-name app_id) AND com.tuxlink.app.png
#      (reverse-DNS identifier app_id). Tauri 2.x's actual Wayland app_id is
#      currently the binary name in most runtime configurations, but a future
#      GApplication.application_id setup could route to com.tuxlink.app. Both
#      paths are cheap; installing both covers either dispatch.
#   2. Writes ~/.local/share/applications/{tuxlink,com.tuxlink.app}.desktop —
#      same coverage logic for the .desktop lookup side.
#   3. Refreshes the desktop database + icon cache (best-effort).
#
# Idempotent: re-runs are safe and just re-install the latest files. Adds
# nothing to system paths (/usr/share/...) — userland install only.
#
# Run from the repo root (or any worktree):
#   bash scripts/install-desktop-entry.sh
#
# Tuxlink-mj7i: closed the "taskbar shows default Tauri icon" regression.
# Tuxlink-xcay: hardened with dual-app_id install after operator-reported
# follow-up — Tauri 2.x's actual Wayland app_id was the binary name 'tuxlink',
# not the tauri.conf.json identifier 'com.tuxlink.app' the original install
# assumed (GApplication didn't register on the session bus — confirmed via
# `gdbus list-names --session`).

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
ICON_DIR="${HOME}/.local/share/icons/hicolor"
APPS_DIR="${HOME}/.local/share/applications"

# Both app_id values we install under. The actual Wayland app_id Tauri sets at
# runtime is currently 'tuxlink' (binary name) — wf-panel-pi and other
# wlr-foreign-toplevel-management panels look up icons via that. The
# reverse-DNS 'com.tuxlink.app' covers a future GApplication.application_id
# setup if Tauri changes its default. Both are cheap; installing both removes
# the failure-mode of guessing wrong.
APP_IDS=("tuxlink" "com.tuxlink.app")

if [ ! -d "${ICONS_SRC}" ]; then
  printf '✗ %s not found — are you in the tuxlink repo?\n' "${ICONS_SRC}" >&2
  exit 1
fi

# --- Step 1: install icons into hicolor theme under BOTH names ---------------
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
  mkdir -p "${dest_dir}"
  for app_id in "${APP_IDS[@]}"; do
    dest="${dest_dir}/${app_id}.png"
    cp -f "${src}" "${dest}"
    printf '✓ installed %s → %s\n' "${size}" "${dest}"
  done
done

# --- Step 2: install BOTH .desktop files -------------------------------------
# Canonical sources live at scripts/${app_id}.desktop — same files
# tauri.conf.json's bundle.linux.deb.files will ship into production .debs.
mkdir -p "${APPS_DIR}"
for app_id in "${APP_IDS[@]}"; do
  src_desktop="${REPO_ROOT}/scripts/${app_id}.desktop"
  dest_desktop="${APPS_DIR}/${app_id}.desktop"
  if [ ! -f "${src_desktop}" ]; then
    printf '! source .desktop missing — skipping %s\n' "${src_desktop}" >&2
    continue
  fi
  cp -f "${src_desktop}" "${dest_desktop}"
  chmod 644 "${dest_desktop}"
  printf '✓ installed %s → %s\n' "${src_desktop}" "${dest_desktop}"
done

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
printf '  - On labwc + wf-panel-pi (Raspberry Pi): restart wf-panel-pi to refresh window-list icons:\n'
printf '      pkill wf-panel-pi   # lwrespawn restarts it automatically\n'
printf '  - On Sway / other wlr-based compositors: usually pick up new icons on window-map\n'
printf '  - Verify .desktop files: ls -la %s/{tuxlink,com.tuxlink.app}.desktop\n' "${APPS_DIR}"
printf '  - Verify icons installed: ls %s/*/apps/{tuxlink,com.tuxlink.app}.png\n' "${ICON_DIR}"
printf '  - To inspect the actual Wayland app_id of the running window (if wlrctl is\n'
printf '    installed):\n'
printf '      sudo apt install -y wlrctl   # one-shot install on Debian/Ubuntu/Raspbian\n'
printf '      wlrctl toplevel list         # shows app_id of every open window\n'
printf '    If the tuxlink window'\''s app_id is neither "tuxlink" nor\n'
printf '    "com.tuxlink.app", that'\''s the missing dispatch — file a bd issue with\n'
printf '    the actual app_id so the install script can be extended.\n'

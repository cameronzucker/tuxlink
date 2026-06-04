#!/usr/bin/env bash
# install-desktop-entry.sh — register Tuxlink with the Linux desktop so the
# taskbar / dock / app-switcher show the Tuxlink icon instead of the default
# missing-app icon (dev-mode path; .deb installs use the bundler's auto-gen
# in /usr/share/...).
#
# What it does:
#   1. Copies src-tauri/icons/* into ~/.local/share/icons/hicolor/<size>/apps/
#      under the name tuxlink.png (binary-name app_id, matching Tauri 2.x's
#      default runtime GApplication.application_id = g_get_prgname()).
#   2. Writes ~/.local/share/applications/tuxlink.desktop from
#      scripts/tuxlink.desktop — the canonical source also referenced by
#      tauri.conf.json's bundle.linux.deb.files.
#   3. Refreshes the desktop database + icon cache (best-effort).
#
# Idempotent: re-runs are safe and just re-install the latest files. Adds
# nothing to system paths (/usr/share/...) — userland install only.
#
# Run from the repo root (or any worktree):
#   bash scripts/install-desktop-entry.sh
#
# Tuxlink-mj7i: closed the "taskbar shows default Tauri icon" dev-mode regression.
# Tuxlink-xcay: shipped a dual-app_id workaround (tuxlink + com.tuxlink.app
# naming) when the runtime Wayland app_id was diagnosed as binary-name only.
# Tuxlink-mpds (2026-06-04): retired the dual-install once source-read +
# Codex consult confirmed binary-name addressing is the architecturally correct
# lane for tuxlink's bundle config (no enableGTKAppId, no Rust-side override).

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

# Single app_id lane: binary name. This matches Tauri 2.x's default runtime
# GApplication.application_id (derived from g_get_prgname() when enableGTKAppId
# is unset — i.e. the project's current config). Wayland xdg_toplevel.app_id
# and X11 WM_CLASS both resolve to "tuxlink", and the bundler-installed
# .desktop / icons (production) use the same name.
APP_ID="tuxlink"

if [ ! -d "${ICONS_SRC}" ]; then
  printf '✗ %s not found — are you in the tuxlink repo?\n' "${ICONS_SRC}" >&2
  exit 1
fi

# --- Step 1: install icons into hicolor theme --------------------------------
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
  dest="${dest_dir}/${APP_ID}.png"
  cp -f "${src}" "${dest}"
  printf '✓ installed %s → %s\n' "${size}" "${dest}"
done

# --- Step 2: install tuxlink.desktop -----------------------------------------
# Canonical source lives at scripts/tuxlink.desktop — same file
# tauri.conf.json's bundle.linux.deb.files ships into production .debs.
mkdir -p "${APPS_DIR}"
src_desktop="${REPO_ROOT}/scripts/${APP_ID}.desktop"
dest_desktop="${APPS_DIR}/${APP_ID}.desktop"
if [ ! -f "${src_desktop}" ]; then
  printf '✗ source .desktop missing: %s\n' "${src_desktop}" >&2
  exit 1
fi
cp -f "${src_desktop}" "${dest_desktop}"
chmod 644 "${dest_desktop}"
printf '✓ installed %s → %s\n' "${src_desktop}" "${dest_desktop}"

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
  # absent index.theme is normal noise.
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
printf '  - Verify .desktop file: ls -la %s/tuxlink.desktop\n' "${APPS_DIR}"
printf '  - Verify icons installed: ls %s/*/apps/tuxlink.png\n' "${ICON_DIR}"
printf '  - To inspect the actual Wayland app_id of the running window (if wlrctl is\n'
printf '    installed):\n'
printf '      sudo apt install -y wlrctl   # one-shot install on Debian/Ubuntu/Raspbian\n'
printf '      wlrctl toplevel list         # shows app_id of every open window\n'
printf '    Expected: tuxlink. If different, that means Tauri changed its default\n'
printf '    GTK app_id behavior — file a bd issue to re-evaluate (bundler\n'
printf '    integration would also need to track).\n'

#!/usr/bin/env bash
# build-basemap-bundle.sh — produce the bundled vector-basemap assets for tuxlink.
#
# tuxlink-ndi4, plan A8/A10/A12. OUT-OF-BAND (not PR CI): operator tooling that
# emits provenanced artifacts the app bundles. It does NOT download the planet —
# `pmtiles extract` reads the pinned REMOTE build over HTTP Range, fetching only
# the requested tiles (world z0–6 ≈ 45 MB, seconds). The ~120 GB is the remote
# source, never pulled whole. Needs the go-pmtiles CLI + the
# protomaps/basemaps-assets fonts+sprites. Treat the output like
# resources/propagation/ssn-forecast.json: provenanced, reproducible.
#
# Outputs to TWO destinations (two webview origins — see the Paths block below):
#   src-tauri/resources/basemap/   (Tauri RESOURCE, served via tile://pmtiles/world)
#     world-z0-6.pmtiles           world overview, zoom 0–6 (~30–60 MB)
#     provenance.json              every pin + sha256 (audit trail)
#   public/basemap/                (frontend 'self' origin → dist, served at /basemap/…)
#     glyphs/<fontstack>/<range>.pbf  Noto Sans Regular/Medium/Italic, Latin ranges
#     sprites/light.{json,png,@2x.png}
#     sprites/dark.{json,png,@2x.png}
#
# After a successful run, ensure src-tauri/tauri.conf.json `bundle.resources` has:
#   "resources/basemap/**/*"
# (glyphs/sprites need NO bundle.resources entry — they ride the frontend dist.)
#
# Usage:
#   scripts/build-basemap-bundle.sh --planet /path/to/planet.pmtiles
#   scripts/build-basemap-bundle.sh                # downloads the pinned build
set -euo pipefail

# ── PINS (operator-confirmable) ─────────────────────────────────────────────
# One planet build hash for the bundle AND every catalog pack (plan A10): a
# divergent schema between the overview and a region pack causes the R7
# compositing seam to blank. The 13-id vector schema is enforced at runtime by
# src-tauri/src/basemap/validate.rs against this build's metadata.
PLANET_BUILD="${PLANET_BUILD:-20260608}"              # Protomaps daily build id (YYYYMMDD); builds rotate ~monthly and old ones 404 — 20260608 is the live build pinned by region-manifest.json (D1)
PLANET_URL_BASE="${PLANET_URL_BASE:-https://build.protomaps.com}"
# protomaps/basemaps-assets release tag for fonts + sprites (schema v5 era).
BASEMAPS_ASSETS_REF="${BASEMAPS_ASSETS_REF:-main}"
ASSETS_RAW="https://raw.githubusercontent.com/protomaps/basemaps-assets/${BASEMAPS_ASSETS_REF}"

# World overview coverage.
MAXZOOM="${MAXZOOM:-6}"
BBOX="${BBOX:--180,-85,180,85}"                       # whole world (web-mercator lat clamp)

# Fontstacks the pinned @protomaps/basemaps@5 LIGHT flavor references (verified
# via `text-font` on the generated style — Noto Sans Regular/Medium/Italic).
# Dark reuses the same glyphs (color is in the style, not the glyph PBF).
FONTSTACKS=("Noto Sans Regular" "Noto Sans Medium" "Noto Sans Italic")
# Latin-only is the documented EmComm default (CJK glyph sets balloon the bundle
# by hundreds of MB). Latin + Latin-1 Supplement + Latin Extended-A/B.
GLYPH_RANGES=("0-255" "256-511" "512-767" "768-1023")
# Per-flavor sprite sheets (icons are raster PNG — NOT slot-color-derivable, so
# dark needs its own sheet; plan A7).
SPRITE_FLAVORS=("light" "dark")

# ── Paths ────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Two destinations, two webview origins (do NOT collapse them):
#   OUT_DIR  — Tauri RESOURCE dir. world-z0-6.pmtiles is resolved in Rust via
#              BaseDirectory::Resource and served over the tile://pmtiles/world
#              206 seam. Requires "resources/basemap/**/*" in bundle.resources.
#   GLYPH_SPRITE_DIR — the frontend 'self' origin (public/ → dist/). MapLibre
#              fetches glyphs ({fontstack}/{range}.pbf) and the sprite sheet as
#              whole-file GETs at /basemap/... — these are NOT byte-range pmtiles
#              and are NOT Tauri resources; they ride the frontend dist. See
#              src/map/basemapStyle.ts (GLYPHS_URL = '/basemap/glyphs/…').
OUT_DIR="${REPO_ROOT}/src-tauri/resources/basemap"
GLYPH_SPRITE_DIR="${REPO_ROOT}/public/basemap"
PLANET_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --planet) PLANET_PATH="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# ── Preflight ─────────────────────────────────────────────────────────────────
command -v pmtiles >/dev/null 2>&1 || {
  echo "ERROR: the 'pmtiles' CLI is required (https://github.com/protomaps/go-pmtiles)." >&2
  exit 1
}
command -v sha256sum >/dev/null 2>&1 || { echo "ERROR: sha256sum required." >&2; exit 1; }
PMTILES_VERSION="$(pmtiles version 2>/dev/null || echo unknown)"

mkdir -p "${OUT_DIR}" "${GLYPH_SPRITE_DIR}/glyphs" "${GLYPH_SPRITE_DIR}/sprites"

# ── 1. World z0–6 overview ─────────────────────────────────────────────────────
# `pmtiles extract` reads the source over HTTP Range, fetching ONLY the tiles in
# the requested zoom/bbox — NOT the whole planet. World z0–6 is ~45 MB transferred
# in seconds. The ~120 GB pinned build is the REMOTE source, never downloaded
# whole. Pass --planet for an already-local source (optional); otherwise extract
# straight from the pinned remote build over Range.
PLANET_SRC="${PLANET_PATH:-${PLANET_URL_BASE}/${PLANET_BUILD}.pmtiles}"
echo ">> extracting world z0-${MAXZOOM} from ${PLANET_SRC} (HTTP Range — tens of MB, NOT the full planet)…"
pmtiles extract "${PLANET_SRC}" "${OUT_DIR}/world-z0-6.pmtiles" \
  --maxzoom="${MAXZOOM}" --bbox="${BBOX}"

# ── 2. Glyphs (frontend 'self' origin under public/basemap, NOT a Tauri resource
#       and NOT via pmtiles_read_range — glyphs are {fontstack}/{range}-keyed) ──
for stack in "${FONTSTACKS[@]}"; do
  enc="${stack// /%20}"
  mkdir -p "${GLYPH_SPRITE_DIR}/glyphs/${stack}"
  for range in "${GLYPH_RANGES[@]}"; do
    echo ">> glyph ${stack} ${range}"
    curl -fSL "${ASSETS_RAW}/fonts/${enc}/${range}.pbf" \
      -o "${GLYPH_SPRITE_DIR}/glyphs/${stack}/${range}.pbf"
  done
done

# ── 3. Sprites (per flavor; dark is a distinct sheet, plan A7; frontend origin) ─
for flavor in "${SPRITE_FLAVORS[@]}"; do
  for ext in json png; do
    curl -fSL "${ASSETS_RAW}/sprites/v4/${flavor}.${ext}" \
      -o "${GLYPH_SPRITE_DIR}/sprites/${flavor}.${ext}"
  done
  curl -fSL "${ASSETS_RAW}/sprites/v4/${flavor}@2x.png" \
    -o "${GLYPH_SPRITE_DIR}/sprites/${flavor}@2x.png" || true   # @2x optional
done

# ── 4. Provenance ───────────────────────────────────────────────────────────────
WORLD_SHA="$(sha256sum "${OUT_DIR}/world-z0-6.pmtiles" | cut -d' ' -f1)"
WORLD_BYTES="$(stat -c%s "${OUT_DIR}/world-z0-6.pmtiles")"
BUILT_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > "${OUT_DIR}/provenance.json" <<JSON
{
  "artifact": "tuxlink vector basemap bundle",
  "built_at": "${BUILT_AT}",
  "planet_build": "${PLANET_BUILD}",
  "planet_url_base": "${PLANET_URL_BASE}",
  "pmtiles_cli_version": "${PMTILES_VERSION}",
  "bbox": "${BBOX}",
  "maxzoom": ${MAXZOOM},
  "basemaps_assets_ref": "${BASEMAPS_ASSETS_REF}",
  "fontstacks": ["Noto Sans Regular", "Noto Sans Medium", "Noto Sans Italic"],
  "glyph_ranges": ["0-255", "256-511", "512-767", "768-1023"],
  "sprite_flavors": ["light", "dark"],
  "world_z0_6": { "sha256": "${WORLD_SHA}", "bytes": ${WORLD_BYTES} }
}
JSON

echo ""
echo "DONE."
echo "  Tauri resource:  ${OUT_DIR}/world-z0-6.pmtiles  ${WORLD_BYTES} bytes  sha256 ${WORLD_SHA}"
echo "  Frontend 'self': ${GLYPH_SPRITE_DIR}/{glyphs,sprites}/  (served at /basemap/… from dist)"
echo ""
echo "NEXT: ensure \"resources/basemap/**/*\" is in src-tauri/tauri.conf.json bundle.resources"
echo "      (glyphs/sprites ride the frontend dist automatically — no bundle.resources entry)."
echo "      A packaged build then registers tile://pmtiles/world at startup and labels resolve."

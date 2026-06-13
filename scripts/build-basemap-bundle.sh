#!/usr/bin/env bash
# build-basemap-bundle.sh — produce the bundled vector-basemap assets for tuxlink.
#
# tuxlink-ndi4, plan A8/A10/A12. OUT-OF-BAND: this needs the ~120 GB Protomaps
# planet PMTiles (or a pinned planet build) and the protomaps/basemaps-assets
# fonts+sprites. It does NOT run in PR CI — it is operator tooling, run on a
# machine with the planet available, that emits provenanced artifacts the app
# bundles. Treat the output like resources/propagation/ssn-forecast.json:
# provenanced, reproducible, not a mystery blob.
#
# Outputs into src-tauri/resources/basemap/:
#   world-z0-6.pmtiles            world overview, zoom 0–6 (~30–60 MB)
#   glyphs/<fontstack>/<range>.pbf  Noto Sans Regular/Medium/Italic, Latin ranges
#   sprites/light.{json,png,@2x.png}
#   sprites/dark.{json,png,@2x.png}
#   provenance.json               every pin + sha256 (audit trail)
#
# After a successful run, add to src-tauri/tauri.conf.json `bundle.resources`:
#   "resources/basemap/**/*"
# (kept OUT of the repo until this script has run, because tauri build errors on
# a resource path that resolves to nothing.)
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
PLANET_BUILD="${PLANET_BUILD:-20240801}"              # Protomaps daily build id (YYYYMMDD)
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
OUT_DIR="${REPO_ROOT}/src-tauri/resources/basemap"
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

mkdir -p "${OUT_DIR}/glyphs" "${OUT_DIR}/sprites"

# ── 1. World z0–6 overview ─────────────────────────────────────────────────────
if [[ -z "${PLANET_PATH}" ]]; then
  PLANET_PATH="${OUT_DIR}/.planet-${PLANET_BUILD}.pmtiles"
  if [[ ! -f "${PLANET_PATH}" ]]; then
    echo ">> downloading pinned planet build ${PLANET_BUILD} (~120 GB — this is the out-of-band step)…"
    curl -fSL "${PLANET_URL_BASE}/${PLANET_BUILD}.pmtiles" -o "${PLANET_PATH}"
  fi
fi
echo ">> extracting world z0-${MAXZOOM} from ${PLANET_PATH}…"
pmtiles extract "${PLANET_PATH}" "${OUT_DIR}/world-z0-6.pmtiles" \
  --maxzoom="${MAXZOOM}" --bbox="${BBOX}"

# ── 2. Glyphs (served under the 'self' origin, NOT via pmtiles_read_range) ──────
for stack in "${FONTSTACKS[@]}"; do
  enc="${stack// /%20}"
  mkdir -p "${OUT_DIR}/glyphs/${stack}"
  for range in "${GLYPH_RANGES[@]}"; do
    echo ">> glyph ${stack} ${range}"
    curl -fSL "${ASSETS_RAW}/fonts/${enc}/${range}.pbf" \
      -o "${OUT_DIR}/glyphs/${stack}/${range}.pbf"
  done
done

# ── 3. Sprites (per flavor; dark is a distinct sheet, plan A7) ──────────────────
for flavor in "${SPRITE_FLAVORS[@]}"; do
  for ext in json png; do
    curl -fSL "${ASSETS_RAW}/sprites/v4/${flavor}.${ext}" \
      -o "${OUT_DIR}/sprites/${flavor}.${ext}"
  done
  curl -fSL "${ASSETS_RAW}/sprites/v4/${flavor}@2x.png" \
    -o "${OUT_DIR}/sprites/${flavor}@2x.png" || true   # @2x optional
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
echo "DONE. Wrote ${OUT_DIR}/"
echo "  world-z0-6.pmtiles  ${WORLD_BYTES} bytes  sha256 ${WORLD_SHA}"
echo ""
echo "NEXT: add \"resources/basemap/**/*\" to src-tauri/tauri.conf.json bundle.resources,"
echo "      then a packaged build will register tile://pmtiles/world at startup."

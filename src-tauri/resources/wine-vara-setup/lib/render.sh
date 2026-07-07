# shellcheck shell=bash
# lib/render.sh — presentation layer over pipeline events.
#
# WV_RENDER selects the face: "text" (default, human) or "json" (JSONL, for
# programmatic consumers such as Tuxlink's setup wizard). The JSON shape is the
# frozen integration contract — see docs/tuxlink-integration.md. Do not change
# field names/order without bumping WV_CONTRACT.
#
# Requires lib/checkpoints.sh (wv_label).

WV_CONTRACT=1
: "${WV_RENDER:=text}"

# Escape a string for embedding in a JSON double-quoted value. Handles the
# control characters that appear in captured stderr (newline/CR/tab) — a raw
# newline in a value would otherwise emit invalid JSON and break JSONL framing.
wv_json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"       # backslash first
  s="${s//\"/\\\"}"       # double-quote
  s="${s//$'\n'/\\n}"     # newline
  s="${s//$'\r'/\\r}"     # carriage return
  s="${s//$'\t'/\\t}"     # tab
  printf '%s' "$s"
}

# Emit the contract handshake (json only; text is silent).
wv_hello() {
  [ "$WV_RENDER" = json ] && printf '{"event":"hello","contract":%d}\n' "$WV_CONTRACT"
  return 0
}

# wv_emit <id> <index> <total> <state> [detail]
wv_emit() {
  local id="$1" index="$2" total="$3" state="$4" detail="${5:-}"
  if [ "$WV_RENDER" = json ]; then
    printf '{"event":"checkpoint","id":"%s","index":%d,"total":%d,"state":"%s","detail":"%s"}\n' \
      "$id" "$index" "$total" "$state" "$(wv_json_escape "$detail")"
  else
    local mark
    case "$state" in
      running) mark="…" ;;
      done)    mark="OK" ;;
      skipped) mark="— (already set up)" ;;
      failed)  mark="FAILED" ;;
      *)       mark="$state" ;;
    esac
    printf '[%d/%d] %s %s%s\n' "$index" "$total" "$(wv_label "$id")" "$mark" \
      "${detail:+ — $detail}"
  fi
}

# wv_summary <ok:true|false> [prefix] [vara_version]
wv_summary() {
  local ok="$1" prefix="${2:-}" ver="${3:-}"
  if [ "$WV_RENDER" = json ]; then
    printf '{"event":"summary","ok":%s,"prefix":"%s","vara_version":"%s"}\n' \
      "$ok" "$(wv_json_escape "$prefix")" "$(wv_json_escape "$ver")"
  else
    if [ "$ok" = true ]; then
      printf 'VARA HF is set up at %s%s\n' "$prefix" "${ver:+ ($ver)}"
    else
      printf 'Setup did not complete. Run: wine-vara-setup doctor\n'
    fi
  fi
}

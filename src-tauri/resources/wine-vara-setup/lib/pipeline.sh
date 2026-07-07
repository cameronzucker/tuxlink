# shellcheck shell=bash
# lib/pipeline.sh — the install runner and read-only commands.
# Requires env.sh, checkpoints.sh, render.sh sourced first.

# Run the full install pipeline: detect → skip / do → emit, per checkpoint.
# Honors WV_AUTOSTART (1 to install the login unit) and WV_INSTALLER (path).
# Emits the JSONL/text stream; returns 0 iff every required checkpoint is green.
wv_install() {
  wv_preflight || return 1   # guard ALL callers (CLI, menu, consumers), not just the CLI
  wv_wineenv
  wv_hello
  local total ok=true index=0 id
  # shellcheck disable=SC2086  # intentional split to count words
  total="$(set -- $WV_CHECKPOINTS; echo $#)"
  for id in $WV_CHECKPOINTS; do
    index=$((index + 1))
    wv_emit "$id" "$index" "$total" running

    # autostart is opt-in; skip unless explicitly requested.
    if [ "$id" = autostart ] && [ "${WV_AUTOSTART:-}" != 1 ]; then
      wv_emit "$id" "$index" "$total" skipped "not requested"; continue
    fi
    # verify is an action (launch), never skipped by detection.
    if [ "$id" != verify ] && "detect_$id"; then
      wv_emit "$id" "$index" "$total" skipped; continue
    fi

    if err="$("do_$id" 2>&1 >/dev/null)"; then
      wv_emit "$id" "$index" "$total" "done"
    else
      wv_emit "$id" "$index" "$total" failed "$err"
      ok=false; break
    fi
  done

  local ver=""
  [ "$ok" = true ] && ver="$(wv_vara_version 2>/dev/null || true)"
  wv_summary "$ok" "$(wv_prefix)" "$ver"
  [ "$ok" = true ]
}

# Best-effort VARA version string (empty if not resolvable offline).
wv_vara_version() {
  local dat
  dat="$(wv_prefix)/drive_c/VARA HF"
  [ -d "$dat" ] || return 1
  echo "VARA HF"   # exact version only known from a live VERSION handshake
}

# Read-only status: emit each installable checkpoint's state; exit 0 iff the
# core set (deps prefix vara vb6 ocx) is all satisfied. Never mutates, no network.
wv_status() {
  wv_hello
  local total ok=true index=0 id core="deps prefix vara vb6 ocx"
  # shellcheck disable=SC2086  # intentional split to count words
  total="$(set -- $WV_CHECKPOINTS; echo $#)"
  for id in $WV_CHECKPOINTS; do
    index=$((index + 1))
    if [ "$id" = verify ]; then
      wv_emit "$id" "$index" "$total" skipped "run-time check only"; continue
    fi
    if "detect_$id"; then
      wv_emit "$id" "$index" "$total" "done"
    else
      wv_emit "$id" "$index" "$total" failed "not set up"
      case " $core " in *" $id "*) ok=false ;; esac
    fi
  done
  wv_summary "$ok" "$(wv_prefix)" ""
  [ "$ok" = true ]
}

# Print an actionable diagnosis keyed on which detector fails (the DESIGN map).
wv_doctor() {
  wv_preflight || return 1
  local any=1
  detect_deps   || { echo "[deps] WINE/winetricks missing — install: sudo apt install wine winetricks"; any=0; }
  detect_prefix || { echo "[prefix] no WINE prefix — run: wine-vara-setup install"; any=0; }
  detect_vara   || { echo "[vara] VARA.exe not found — supply the installer: --installer <path>"; any=0; }
  detect_vb6    || { echo "[vb6] MSVBVM60 missing — VARA would exit with c0000135. Fix: winetricks vb6run"; any=0; }
  detect_ocx    || { echo "[ocx] OCX controls unregistered — VARA dies ~18s with com_get_class_object {248dd896-…}. Fix: install step 'ocx'"; any=0; }
  [ "$any" = 1 ] && echo "All checkpoints satisfied. If VARA still fails, check audio routing and a clear channel."
  return 0
}

# Bring VARA up in the foreground-ish (backgrounded), report the listening ports.
wv_launch() {
  wv_wineenv
  detect_vara || { echo "VARA is not installed. Run: wine-vara-setup install" >&2; return 1; }
  local pid; pid="$(wv_start_vara)"
  echo "VARA started (pid $pid) — WINEPREFIX=$(wv_prefix)"
  if [ "${1:-}" = "--daemon" ]; then
    echo "$pid" >"$(wv_prefix)/.vara.pid"
  else
    if wv_wait_ports; then
      echo "Listening on 127.0.0.1:8300/8301"
    else
      echo "Ports did not come up in time" >&2
      wv_stop_vara "$pid"
      return 1
    fi
    wait "$pid" 2>/dev/null || true
  fi
}

# Stop a daemonized VARA. Validates the recorded PID actually belongs to a
# wine/VARA process before signalling, so a stale/reused/edited pidfile cannot
# make us kill an unrelated process the user owns.
wv_stop() {
  local pidfile pid; pidfile="$(wv_prefix)/.vara.pid"
  if [ ! -f "$pidfile" ]; then echo "no daemon pid recorded"; return 0; fi
  pid="$(cat "$pidfile")"
  if [[ "$pid" =~ ^[0-9]+$ ]] && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -qiE 'wine|VARA'; then
    wv_stop_vara "$pid"; rm -f "$pidfile"; echo "stopped"
  else
    rm -f "$pidfile"; echo "recorded pid is stale or not a VARA process; not killing" >&2; return 1
  fi
}

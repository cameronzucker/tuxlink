# shellcheck shell=bash
# lib/menu.sh — whiptail interactive face (a raspi-config-style menu).
# Requires env.sh, checkpoints.sh, render.sh, pipeline.sh sourced first.
# Not driven in CI (whiptail needs a terminal); guarded by WV_NO_WHIPTAIL.

# Map a checkpoint index/total to a 0..100 gauge percentage.
wv_gauge_percent() { echo $(( $1 * 100 / $2 )); }

# Translate the JSONL install stream on stdin into whiptail --gauge protocol.
wv_jsonl_to_gauge() {
  local line id index total state label
  while IFS= read -r line; do
    case "$line" in *'"event":"checkpoint"'*) : ;; *) continue ;; esac
    id="$(sed -n 's/.*"id":"\([^"]*\)".*/\1/p' <<<"$line")"
    index="$(sed -n 's/.*"index":\([0-9]*\).*/\1/p' <<<"$line")"
    total="$(sed -n 's/.*"total":\([0-9]*\).*/\1/p' <<<"$line")"
    state="$(sed -n 's/.*"state":"\([^"]*\)".*/\1/p' <<<"$line")"
    [ "$state" = running ] || continue
    label="$(wv_label "$id" 2>/dev/null || echo "$id")"
    printf 'XXX\n%d\n%s\nXXX\n' "$(wv_gauge_percent "$index" "$total")" "$label"
  done
}

wv_menu_install() {
  local exe
  whiptail --title "VARA HF setup" --msgbox \
    "VARA HF is proprietary freeware and cannot be bundled.\n\nDownload it in your web browser from the author page (rosmodem / EA5HVK) or the Winlink software page, then point this tool at the downloaded .exe on the next screen." \
    14 72 || return 0
  exe="$(whiptail --title "Select the VARA installer" --fselect "$HOME/Downloads/" 20 76 3>&1 1>&2 2>&3)" || return 0
  if [ -z "$exe" ] || [ ! -e "$exe" ]; then whiptail --msgbox "No installer selected." 8 50; return 0; fi

  if WV_INSTALLER="$exe" WV_AUTOSTART=1 WV_RENDER=json wv_install \
       | wv_jsonl_to_gauge \
       | whiptail --title "Setting up VARA HF" --gauge "Preparing…" 8 72 0; then
    whiptail --msgbox "VARA HF is set up and will start on login." 8 60
  else
    whiptail --msgbox "Setup did not complete. Choose 'Doctor' to diagnose." 8 60
  fi
}

wv_menu() {
  [ -n "${WV_NO_WHIPTAIL:-}" ] && return 0
  local choice
  while :; do
    choice="$(whiptail --title "wine-vara-setup" --menu "Set up VARA HF under WINE" 16 64 6 \
      install "Install / configure VARA HF" \
      status  "Check what is set up" \
      launch  "Start VARA now" \
      doctor  "Diagnose a problem" \
      quit    "Exit" \
      3>&1 1>&2 2>&3)" || return 0
    case "$choice" in
      install) wv_menu_install ;;
      status)  whiptail --title "Status" --msgbox "$(WV_RENDER=text wv_status 2>&1)" 18 72 ;;
      launch)  whiptail --title "Launch" --msgbox "$(wv_launch --daemon 2>&1)" 12 66 ;;
      doctor)  whiptail --title "Doctor" --msgbox "$(wv_doctor 2>&1)" 20 76 ;;
      quit|"") return 0 ;;
    esac
  done
}

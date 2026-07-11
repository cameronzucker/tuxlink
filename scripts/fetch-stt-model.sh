#!/usr/bin/env bash
# Fetch the offline Whisper STT model used by the off-air WWV/WWVH space-weather
# decode feature (tuxlink-xscum).
#
# This is a ONE-TIME SETUP step that needs internet ONCE. The decode feature's
# RUNTIME is fully off-air — the model lives in the local data dir and is never
# fetched again. For a genuinely air-gapped install, copy the model file onto the
# target machine by hand (see MANUAL PLACEMENT below) instead of running this.
#
# Idempotent: if the model is already present and its SHA-256 matches, this exits
# immediately without re-downloading.
set -euo pipefail

MODEL_FILENAME="ggml-base.en-q5_1.bin"
# whisper.cpp base.en, q5_1-quantized (~57 MB). Pinned SHA-256 (verified 2026-07-11).
MODEL_SHA256="4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_FILENAME}"

DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"
MODEL_DIR="${DATA_HOME}/tuxlink/models"
MODEL_PATH="${MODEL_DIR}/${MODEL_FILENAME}"

verify_sha() { echo "${MODEL_SHA256}  ${1}" | sha256sum --check --status; }

if [ -f "${MODEL_PATH}" ] && verify_sha "${MODEL_PATH}"; then
  echo "STT model already present and verified:"
  echo "  ${MODEL_PATH}"
  exit 0
fi

mkdir -p "${MODEL_DIR}"
echo "Downloading ${MODEL_FILENAME} (~57 MB) to ${MODEL_DIR} ..."
tmp="${MODEL_PATH}.tmp.$$"
trap 'rm -f "${tmp}"' EXIT
curl -fSL "${MODEL_URL}" -o "${tmp}"

if ! verify_sha "${tmp}"; then
  echo "ERROR: SHA-256 mismatch — refusing to install a corrupt/unexpected model." >&2
  echo "Expected ${MODEL_SHA256}" >&2
  echo "Got      $(sha256sum "${tmp}" | cut -d' ' -f1)" >&2
  exit 1
fi

mv -f "${tmp}" "${MODEL_PATH}"
trap - EXIT
echo "Installed and verified:"
echo "  ${MODEL_PATH}"
echo
echo "MANUAL PLACEMENT (air-gapped installs): copy ${MODEL_FILENAME} to the path"
echo "above on the target machine, or set wwv_offair.model_path in config.json to"
echo "wherever you placed it. Verify with: sha256sum ${MODEL_FILENAME}"

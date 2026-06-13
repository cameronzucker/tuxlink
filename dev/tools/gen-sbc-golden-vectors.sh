#!/usr/bin/env bash
# Regenerate SBC golden vectors (tuxlink-vgvn). Deterministic 1kHz sine PCM ->
# ffmpeg sbc encode with the UV-Pro/benlink params (32kHz mono, bitpool 16, 8
# subbands, 16 blocks, Loudness alloc — frame header 9c 71 10). No radio needed.
set -euo pipefail
OUT="${1:-src-tauri/src/winlink/ax25/uvpro/audio/testdata}"
python3 - "$OUT/sine1k_32k_mono.pcm" <<'PY'
import struct, math, sys
n, sr, f = 4096, 32000, 1000.0
with open(sys.argv[1], 'wb') as o:
    for i in range(n):
        o.write(struct.pack('<h', int(0.5*32767*math.sin(2*math.pi*f*i/sr))))
PY
ffmpeg -hide_banner -loglevel error -y -f s16le -ar 32000 -ac 1 \
  -i "$OUT/sine1k_32k_mono.pcm" -c:a sbc -global_quality 1888 -f sbc \
  "$OUT/sine1k_32k_mono.sbc"
echo "regenerated golden vectors in $OUT"

# xtask

Workspace-internal build helpers for tuxlink.

## Binaries

### `gen-corpus`

Generates a synthetic JSONL event corpus for zstd dictionary training.
Output: `dev/log-corpus-synthetic/*.jsonl` (gitignored).

Combines:
- Templated synthetic event sequences (dial attempts, B2F handshakes, modem
  commands, AX.25 frames, env-probe outputs)
- Real-string fixtures from `dev/log-corpus-fixtures/` (operator-curated,
  committed; stderr captures from gnome-keyring / kwallet / KeePassXC /
  PipeWire / ALSA / VARA / ARDOP / BlueZ)

Run: `cargo --manifest-path xtask/Cargo.toml run --bin gen-corpus -- --output dev/log-corpus-synthetic/`

### `train-log-dict`

Trains a zstd dictionary from a corpus directory. Outputs the dictionary
asset bundled into the tuxlink binary via `include_bytes!`.

Run: `cargo --manifest-path xtask/Cargo.toml run --bin train-log-dict -- --input dev/log-corpus-synthetic/ --output src-tauri/assets/logging/tuxlink-events-v1.zdict --size-kb 16`

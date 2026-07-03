#!/usr/bin/env bash
# pod_bootstrap.sh — stand up a fresh RunPod CUDA pod for the Elmer eval / gold-gen
# / training runs, in one command. Idempotent: re-run any time (skips installed
# deps + already-pulled models; re-syncs the harness).
#
# The recurring RunPod tax this automates: apt zstd+rsync -> ollama install+serve
# -> harness rsync -> model pulls. Migrating hardware (A100 -> H200) is then just:
#   1) create the pod, 2) append the SSH key once (this script prints the exact
#   command if SSH is refused), 3) run this script with the new host/port.
#
# Usage:
#   ./pod_bootstrap.sh <host> <port> [options]
# Options:
#   --key PATH        SSH private key (default: ~/.ssh/id_ed25519 = elmer-eval-pi)
#   --models "a,b"    ollama models to pull (default: the full council + student)
#   --no-models       sync harness + deps only, skip model pulls (fast code update)
#   --user U          SSH user (default: root)
#
# Examples:
#   ./pod_bootstrap.sh 154.54.102.37 18484                 # full setup
#   ./pod_bootstrap.sh 1.2.3.4 22 --no-models              # just push a harness update
#   ./pod_bootstrap.sh 1.2.3.4 22 --models gpt-oss:20b     # training-only pod
set -euo pipefail

if [[ $# -lt 2 ]]; then
  grep -E '^#( |$)' "$0" | sed 's/^# \{0,1\}//'   # print the header as help
  exit 1
fi

HOST="$1"; PORT="$2"; shift 2
KEY="$HOME/.ssh/id_ed25519"
USER_="root"
MODELS="gpt-oss:20b,gpt-oss:120b,qwen2.5:72b,llama3.3:70b,nemotron:70b,gemma3:27b"
PULL_MODELS=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --key)       KEY="$2"; shift 2 ;;
    --models)    MODELS="$2"; shift 2 ;;
    --no-models) PULL_MODELS=0; shift ;;
    --user)      USER_="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
  esac
done

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # this = the elmer-distill dir
SSH_OPTS=(-p "$PORT" -i "$KEY" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15 -o ServerAliveInterval=30)
TARGET="$USER_@$HOST"

say() { printf '\n=== %s ===\n' "$1"; }

# 0. SSH preflight — the #1 RunPod failure is the wrong key injected at pod create.
say "0/5 SSH preflight ($TARGET:$PORT)"
if ! ssh "${SSH_OPTS[@]}" "$TARGET" 'echo ok' 2>/dev/null | grep -q ok; then
  echo "SSH refused. RunPod injected the wrong key at pod create."
  echo "Open the pod's RunPod Web Terminal and paste this, then re-run this script:"
  echo
  echo "  mkdir -p ~/.ssh && echo '$(cat "${KEY}.pub")' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
  echo
  echo "(Permanent fix: RunPod Settings -> SSH Public Keys -> add the above, remove any stale key.)"
  exit 1
fi
echo "SSH OK."

# 1. apt deps (ollama's installer needs zstd; rsync for the harness sync)
say "1/5 apt deps (zstd, rsync)"
ssh "${SSH_OPTS[@]}" "$TARGET" 'apt-get update -qq && apt-get install -y -qq zstd rsync 2>&1 | tail -1 || true'

# 2. ollama install + serve
say "2/5 ollama install + serve"
ssh "${SSH_OPTS[@]}" "$TARGET" 'command -v ollama >/dev/null || (curl -fsSL https://ollama.com/install.sh | sh)'
ssh "${SSH_OPTS[@]}" "$TARGET" '
  if ! curl -s http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
    nohup ollama serve >/root/ollama.log 2>&1 & sleep 6
  fi
  echo "ollama $(ollama --version 2>&1 | head -1)"
  curl -s http://127.0.0.1:11434/api/tags >/dev/null && echo "API up" || { echo "API DOWN — check /root/ollama.log"; exit 1; }'

# 3. sync the harness (this elmer-distill dir -> pod:/root/elmer-distill)
say "3/5 sync harness -> $TARGET:/root/elmer-distill"
rsync -az --exclude __pycache__ --exclude '*.pyc' --exclude eval-runs --exclude .pytest_cache \
  -e "ssh ${SSH_OPTS[*]}" "$HARNESS_DIR/" "$TARGET:/root/elmer-distill/"
ssh "${SSH_OPTS[@]}" "$TARGET" 'echo "harness: $(ls /root/elmer-distill/*.py | wc -l) CLIs, $(ls /root/elmer-distill/gate/candidates/*.json | wc -l) gate scenarios"'

# 4. pull models (idempotent; each ~13-65GB — this is the long step)
if [[ "$PULL_MODELS" -eq 1 ]]; then
  say "4/5 pull models ($MODELS)"
  ssh "${SSH_OPTS[@]}" "$TARGET" 'df -h / | tail -1'
  IFS=',' read -ra MS <<< "$MODELS"
  for m in "${MS[@]}"; do
    m="$(echo "$m" | xargs)"   # trim
    echo "--- $m ---"
    ssh "${SSH_OPTS[@]}" "$TARGET" "ollama list | grep -q '$m' && echo 'already present' || ollama pull '$m'"
  done
else
  say "4/5 model pulls SKIPPED (--no-models)"
fi

# 5. verify + next-step hints
say "5/5 ready"
ssh "${SSH_OPTS[@]}" "$TARGET" '
  echo "GPU:"; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
  echo "cores: $(nproc)"; echo "models:"; ollama list | awk "NR>1{print \"  \"\$1\" \"\$3\$4}"'
cat <<EOF

Pod ready. Launch a run (detached, survives disconnect):

  ssh ${SSH_OPTS[*]} $TARGET \\
    'cd /root/elmer-distill && nohup python3 run_council.py \\
       --models gpt-oss:120b,qwen2.5:72b,llama3.3:70b,nemotron:70b,gemma3:27b \\
       --n 5 --out eval-runs/council --max-turns 32 --max-reprompts 1 \\
       > /root/council.log 2>&1 & echo PID \$!'

Pull results back:  ssh ${SSH_OPTS[*]} $TARGET 'cd /root/elmer-distill && tar czf - eval-runs/council' | tar xzf -
Reminder: the pod bills while up — stop it from the RunPod dashboard when idle.
EOF

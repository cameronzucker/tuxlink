#!/bin/bash
# block-spark-oob-serving.sh — PreToolUse Bash hook
#
# Spark inference-cluster serving lifecycle goes through the control-plane
# API / recipe system ONLY (the classifier-kickoff standing rule: served
# "ONLY via a control-plane profile" — no out-of-band containers). This hook
# denies Bash calls that combine a Spark host with a container/serving
# lifecycle verb. Read-only inspection (docker ps/logs/inspect/stats, curl,
# ss) is deliberately NOT blocked — it is how incidents get diagnosed.
#
# Why a hook and not prose: 2026-08-11 incident — an agent session ran test
# models via raw `docker run` on the Sparks, and an authorized `docker stop`
# of the Inkling serving deleted its auto-remove container outright; the
# restore attempt through a stale dev script reproduced the exact pinned
# Triton failure class the blessed recipe exists to avoid (bd tuxlink-fa6x4,
# recipes/*.yaml maintenance warning). Same doctrine as the destructive-git
# hook: prose alone did not prevent it; the hook layer does.
#
# Input:  JSON on stdin with .tool_input.command
# Output: JSON deny on stdout if matched; nothing if clean.
# Exit:   0 always (decision is in the JSON output).

set -u

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')

deny() {
    local reason="$1"
    jq -n --arg reason "$reason" '{
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": $reason
        }
    }'
    exit 0
}

# Spark hosts: the two GB10 nodes by hostname, tailnet name, and fabric IPs.
SPARK_HOSTS='gx10-65aa|twin-bramble|10\.55\.0\.[12]'

# Serving-lifecycle verbs. `docker exec` is included: the blessed flow execs
# the serve process, and doing that out-of-band is exactly the incident.
# Read-only docker verbs (ps, logs, inspect, stats, top, images, df) are
# absent from this list on purpose.
LIFECYCLE='docker[[:space:]]+(run|create|start|stop|restart|rm|kill|pause|unpause|update|exec|compose)\b|vllm[[:space:]]+serve\b|pkill[^|]*vllm|systemctl[[:space:]]+(start|stop|restart)[^|]*docker'

if printf '%s' "$cmd" | grep -qE "$SPARK_HOSTS"; then
    if printf '%s' "$cmd" | grep -qE "$LIFECYCLE"; then
        deny "Spark serving lifecycle is managed via the control-plane API / recipe system ONLY (~/spark-vllm-docker/recipes/*.yaml on the Sparks encode every hard-won pin; kickoff standing rule: no out-of-band containers). Raw docker lifecycle on gx10/twin-bramble/10.55.0.x is banned after the 2026-08-11 incident (an authorized docker stop deleted the live Inkling container). Read-only inspection (docker ps/logs/inspect, curl, ss) is allowed. If serving is broken or you need a model swap, surface it to the operator or use the control-plane API."
    fi
fi

exit 0

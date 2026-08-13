#!/bin/bash
# block-metered-model-apis.sh — PreToolUse Bash hook
#
# Model workloads (judges, adversarial review, subagents, evals) run on
# PLAN-BILLED CLIs (codex exec under ChatGPT auth, claude -p under the
# Claude plan) or LOCAL endpoints (Spark control-plane serving, localhost,
# R2) ONLY. Metered pay-per-token APIs are deny-by-default; a key sitting
# in the OS keyring is NOT authorization to spend it. Approval is per-use,
# from the operator, at the moment of the run. Canonical statement:
# CLAUDE.md §"Model spend — metered APIs are deny-by-default".
#
# Why a hook and not prose/ADR: operator ruling 2026-08-13 after a session
# spun up OpenRouter judges unapproved — an ADR was explicitly rejected as
# the vehicle ("non-binding... agents are already not reading them").
# Single sessions have blown through entire budgets before; unpinned
# OpenRouter serving is wildly inconsistent. Same doctrine as the
# destructive-git and spark-oob-serving hooks: prose alone does not
# prevent it; the hook layer does.
#
# Threat model: good-faith drift (an agent reaching for a metered endpoint
# because it is convenient or familiar), not adversarial evasion —
# deliberate workarounds are already banned by the hooks-are-canonical
# rule. Patterns therefore favor precision over exhaustiveness.
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

# Shared tail for every deny message.
POLICY='Metered model APIs are deny-by-default (CLAUDE.md §"Model spend — metered APIs are deny-by-default"). Approved transports: plan-billed codex exec (ChatGPT auth; stdin-prompt recipe in CLAUDE.md), plan-billed claude -p, or local endpoints (Spark control-plane serving, localhost, R2). A key in the keyring is NOT authorization. If (and only if) the operator has approved THIS specific call, re-run it prefixed with TUXLINK_METERED_API_OVERRIDE=operator-approved (audited to dev/scratch/metered-api-overrides.log). If you hit this because command TEXT merely mentions a metered host (heredoc writing a prompt/doc), write that file with the Write/Edit tools instead of a Bash heredoc.'

# --- Audited override: per-use operator approval ONLY -----------------------
# PreToolUse hooks see the command string, not the command's environment, so
# the override is an inline assignment — which also makes every use loud in
# the transcript. Checked against the RAW command (assignments precede any
# heredoc body).
if printf '%s' "$cmd" | grep -q 'TUXLINK_METERED_API_OVERRIDE=operator-approved'; then
    log_dir="${CLAUDE_PROJECT_DIR:-.}/dev/scratch"
    mkdir -p "$log_dir" 2>/dev/null || true
    printf '%s METERED-OVERRIDE %.300s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$cmd" \
        >> "$log_dir/metered-api-overrides.log" 2>/dev/null || true
    exit 0
fi

# Heredoc-stripped view of the command (same rationale and same conservative
# awk as block-destructive-git.sh): commit messages and prompt files
# legitimately MENTION metered hosts — this hook's own PR does. Only the
# conventional EOF tag is stripped; other tags stay visible and at worst
# cause a spurious deny (reword or use the Write tool), never a miss.
cmd_stripped=$(printf '%s' "$cmd" | awk '
    hd { if ($0 == "EOF") hd = 0; next }
    /<<-?['\''"]?EOF['\''"]?/ { hd = 1; print; next }
    { print }
')

# --- 1) The keyring chokepoint ---------------------------------------------
# Per the no-disk-creds policy the OpenRouter key exists ONLY in the OS
# keyring (service elmer-openrouter). Every metered OpenRouter flow on this
# box must start with this lookup, so denying it cuts the class off at the
# source regardless of which client would have spent the key.
if printf '%s' "$cmd_stripped" | grep -q 'elmer-openrouter'; then
    deny "This command references the elmer-openrouter keyring entry — the OpenRouter API key. $POLICY"
fi

# --- 2) Metered endpoints ---------------------------------------------------
# URL form (scheme present) and network-client form (scheme-less curl-style).
# Local endpoints (inference.twin-bramble.ts.net, localhost, 10.55.0.x) are
# deliberately absent from this list.
METERED_HOSTS='openrouter\.ai|api\.openai\.com|api\.anthropic\.com|generativelanguage\.googleapis\.com|api\.mistral\.ai|api\.together\.(xyz|ai)|api\.groq\.com|api\.deepseek\.com|api\.x\.ai|api\.cohere\.(com|ai)|api\.fireworks\.ai|api\.perplexity\.ai|[a-z0-9-]+\.openai\.azure\.com'
if printf '%s' "$cmd_stripped" | grep -qiE "https?://[^[:space:]\"']*($METERED_HOSTS)"; then
    deny "This command targets a metered pay-per-token model API endpoint. $POLICY"
fi
if printf '%s' "$cmd_stripped" | grep -qiE "\b(curl|wget|http|https|xh|aria2c)\b[^|;&]*($METERED_HOSTS)"; then
    deny "This command points a network client at a metered pay-per-token model API host. $POLICY"
fi

# --- 3) Metered vendor API keys assigned in the command ---------------------
# Assigning one of these inline (VAR=... cmd, or export VAR=...) is how a
# metered client gets armed. INKLING_API_KEY is deliberately absent — the
# Spark endpoint is local and its dummy key is the blessed pattern.
METERED_KEYS='OPENROUTER_API_KEY|OPENAI_API_KEY|ANTHROPIC_API_KEY|GEMINI_API_KEY|MISTRAL_API_KEY|GROQ_API_KEY|TOGETHER_API_KEY|DEEPSEEK_API_KEY|XAI_API_KEY|COHERE_API_KEY|FIREWORKS_API_KEY|PERPLEXITY_API_KEY'
if printf '%s' "$cmd_stripped" | grep -qE "\b($METERED_KEYS)="; then
    deny "This command assigns a metered vendor API key. $POLICY"
fi

# --- 4) codex steered at a metered provider ---------------------------------
# The retired [model_providers.openrouter] block may still exist in
# ~/.codex/config.toml; selecting it (via -c model_provider=..., a profile,
# or any other spelling) re-arms metered routing. codex must appear as a
# standalone token so paths like ~/.codex/config.toml and @openai/codex in
# npx invocations do not false-positive on their own.
if printf '%s' "$cmd_stripped" | grep -qE '(^|[[:space:];|&(])codex([[:space:]]|$)' \
   && printf '%s' "$cmd_stripped" | grep -qi 'openrouter'; then
    deny "This codex invocation references openrouter — the plan-billed ChatGPT auth is the only approved codex transport. $POLICY"
fi

# All checks passed — allow by default (no output).
exit 0

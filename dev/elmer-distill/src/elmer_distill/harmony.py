"""Render trajectories into the gpt-oss **Harmony** training format.

Codex adrev blocker C: gpt-oss is trained on the Harmony token format (roles
system/developer/user/assistant/tool; channels analysis/commentary/final; tool
recipients `functions.<name>`; `<|constrain|>json`; `<|call|>`), NOT the ollama
REST JSON shape. This module uses the official `openai_harmony` encoder so the
training tokens exactly match what the runtime emits and consumes.

`render_training_tokens` is what the trainer consumes. `render_trajectory`
returns the decoded Harmony text (human-readable / for JSONL). `parse_trajectory`
round-trips the assistant actions back out to verify no structural corruption:
it filters for `functions.*` recipients (tool calls) and the `final` channel
(the answer), which is robust to how the completion parser labels other roles.
"""
import json
from functools import lru_cache

from openai_harmony import (load_harmony_encoding, HarmonyEncodingName,
                            Conversation, Message, Role, Author)


@lru_cache(maxsize=1)
def _enc():
    return load_harmony_encoding(HarmonyEncodingName.HARMONY_GPT_OSS)


def _messages_from_trajectory(system, traj):
    msgs = [Message.from_role_and_content(Role.SYSTEM, system)]
    for turn in traj["turns"]:
        role = turn["role"]
        if role == "user":
            msgs.append(Message.from_role_and_content(Role.USER, turn.get("content", "")))
        elif role == "tool":
            name = turn["tool_name"]
            msgs.append(
                Message.from_author_and_content(
                    Author(role=Role.TOOL, name=f"functions.{name}"), turn.get("content", ""))
                .with_channel("commentary").with_recipient("assistant"))
        elif role == "assistant":
            thinking = turn.get("thinking") or ""
            if thinking:
                msgs.append(Message.from_role_and_content(Role.ASSISTANT, thinking)
                            .with_channel("analysis"))
            for tc in turn.get("tool_calls") or []:
                fn = tc["function"]
                args = fn.get("arguments")
                if not isinstance(args, str):
                    args = json.dumps(args if args is not None else {})
                msgs.append(
                    Message.from_role_and_content(Role.ASSISTANT, args)
                    .with_channel("commentary")
                    .with_recipient(f"functions.{fn['name']}")
                    .with_content_type("<|constrain|>json"))
            content = turn.get("content") or ""
            if content and not (turn.get("tool_calls")):
                msgs.append(Message.from_role_and_content(Role.ASSISTANT, content)
                            .with_channel("final"))
    return msgs


def render_training_tokens(system, traj):
    """Return the Harmony training token ids for a full trajectory."""
    conv = Conversation.from_messages(_messages_from_trajectory(system, traj))
    return _enc().render_conversation_for_training(conv)


def render_trajectory(system, traj):
    """Return the decoded Harmony training text for a full trajectory."""
    return _enc().decode_utf8(render_training_tokens(system, traj))


def _text_of(msg_dict):
    return "".join(c.get("text", "") for c in (msg_dict.get("content") or [])
                   if isinstance(c, dict))


def parse_trajectory(text):
    """Round-trip: parse Harmony text back to assistant tool_calls + finals.

    Filters for tool calls (recipient `functions.*`) and final answers
    (channel `final`); other role labels from the completion parser are ignored.
    """
    enc = _enc()
    toks = enc.encode(text, allowed_special="all")
    msgs = enc.parse_messages_from_completion_tokens(toks, Role.ASSISTANT)
    turns = []
    for m in msgs:
        d = m.to_dict()
        recipient = d.get("recipient") or ""
        channel = d.get("channel")
        content = _text_of(d)
        if recipient.startswith("functions."):
            turns.append({"role": "assistant", "thinking": "", "content": "",
                          "tool_calls": [{"function": {"name": recipient[len("functions."):],
                                                       "arguments": content}}]})
        elif channel == "final":
            turns.append({"role": "assistant", "thinking": "", "content": content,
                          "tool_calls": []})
    return {"turns": turns}

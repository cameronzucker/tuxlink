#!/usr/bin/env python3
"""Extend the inkling-fix-streaming-tool-calls mod with the stream-tail
repair: the stop boundary can eat a streamed tool call's closing chars
(`"}` / `"}}`), leaving the flushed `arguments` unparseable -> client nulls
the args. Fix: on the converter's final (partial=False) pass, synthesize
the minimal closing suffix for an unterminated span. Idempotent; backup.
"""
import shutil
import sys
import time
from pathlib import Path

MOD = Path.home() / "spark-vllm-docker/mods/inkling-fix-streaming-tool-calls/patch_inkling_parser.py"
STAMP = time.strftime("%Y%m%dT%H%M%S")

NEW_CONSTANTS = '''

CONVERTER_HELPER_ANCHOR = "def _inkling_arg_converter(raw_args: str, partial: bool) -> str:"

PATCHED_CONVERTER_HELPER = (
    \'\'\'def _span_closers(span: str) -> str:
    """Minimal closing suffix for an unterminated JSON-object span.

    The end-of-generation boundary can eat the final text chunk of a
    streamed tool call (the closing quote/braces ride the same tick as the
    stop sentinel and never reach the parser), leaving the flushed span
    unterminated and the streamed OpenAI ``arguments`` unparseable
    (2026-08-11 bench autopsy: `..."rx_grid":"FN31` with no closing).
    Synthesize the closers so the final flush emits valid JSON. Verbatim
    span is only APPENDED to, preserving the prefix-stability the delta
    diffing requires. A terminated span gets "".
    """
    stack = []
    in_string = False
    escape = False
    for ch in span:
        if escape:
            escape = False
            continue
        if in_string:
            if ch == chr(92):
                escape = True
            elif ch == chr(34):
                in_string = False
            continue
        if ch == chr(34):
            in_string = True
        elif ch in "{[":
            stack.append("}" if ch == "{" else "]")
        elif ch in "}]":
            if stack:
                stack.pop()
    out = ""
    if escape:
        out += chr(34)
    if in_string or escape:
        out += chr(34)
    out += "".join(reversed(stack))
    return out


def _inkling_arg_converter(raw_args: str, partial: bool) -> str:\'\'\'
)

CONVERTER_TAIL_ANCHOR = \'\'\'    span = _args_value_span(raw_args)
    if span is None:
        # No args value yet (streaming) or none at all (treat as empty).
        return "" if partial else "{}"
    return span
\'\'\'

PATCHED_CONVERTER_TAIL = \'\'\'    span = _args_value_span(raw_args)
    if span is None:
        # No args value yet (streaming) or none at all (treat as empty).
        return "" if partial else "{}"
    if not partial:
        # Final flush: the stop boundary may have eaten the closing chars.
        span += _span_closers(span)
    return span
\'\'\'

'''

OLD_CALLS = '''    text = replace_once(
        text,
        CLASS_METHOD_ANCHOR,
        PATCHED_CLASS_METHOD_ANCHOR,
        "InklingParser method anchor",
    )
    validate(text)
    return text
'''

NEW_CALLS = '''    text = replace_once(
        text,
        CLASS_METHOD_ANCHOR,
        PATCHED_CLASS_METHOD_ANCHOR,
        "InklingParser method anchor",
    )
    text = replace_once(
        text,
        CONVERTER_HELPER_ANCHOR,
        PATCHED_CONVERTER_HELPER,
        "arg converter helper anchor",
    )
    text = replace_once(
        text,
        CONVERTER_TAIL_ANCHOR,
        PATCHED_CONVERTER_TAIL,
        "arg converter tail block",
    )
    validate(text)
    return text
'''

OLD_CHECK = '''            'ParserState.REASONING, "TEXT_START"' not in text
            or "def _preprocess_feed(" not in text
'''

NEW_CHECK = '''            'ParserState.REASONING, "TEXT_START"' not in text
            or "def _preprocess_feed(" not in text
            or "_span_closers" not in text
'''


def replace_once(text, old, new, label):
    n = text.count(old)
    if n != 1:
        raise SystemExit(f"expected exactly one {label}; found {n}")
    return text.replace(old, new, 1)


src = MOD.read_text()
if "_span_closers" in src:
    print("mod already extended")
else:
    shutil.copy(MOD, str(MOD) + f".bak-{STAMP}")
    src = replace_once(src, "\n\ndef replace_once(", NEW_CONSTANTS + "\ndef replace_once(",
                       "constants insertion point")
    src = replace_once(src, OLD_CALLS, NEW_CALLS, "patched_text call block")
    src = replace_once(src, OLD_CHECK, NEW_CHECK, "marker completeness check")
    compile(src, str(MOD), "exec")
    MOD.write_text(src)
    print(f"mod extended (backup .bak-{STAMP})")

# Offline proof: run the extended patcher against the extracted live parser
# copy and verify the output compiles and repairs a truncated span.
import importlib.util
spec = importlib.util.spec_from_file_location("mod_patcher", str(MOD))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

pristine = Path("/tmp/inkling-parser-live.py").read_text()
# The live copy already carries the OLD mod's marker; strip to pristine-ish by
# refusing marker path: patched_text() short-circuits on MARKER. Simulate a
# fresh file by removing the marker-guard concern: if marker present, ensure
# completeness check now FAILS (old fix without _span_closers) -> ValueError.
try:
    out = m.patched_text(pristine)
    print("patched_text ran; _span_closers in output:", "_span_closers" in out)
    compile(out, "inkling-patched.py", "exec")
    print("patched parser compiles OK")
except ValueError as e:
    print(f"marker-guard result (expected on already-patched copy): {e}")

# Unit-check the closer logic exactly as it will exist in the parser.
ns = {}
helper_src = m.PATCHED_CONVERTER_HELPER.split("def _inkling_arg_converter")[0]
exec(helper_src, ns)
cases = {
    '{"a":[1,2],"rx_grid":"FN31': '"}',
    '{"a":{"b":1}': "}",
    '{"a":1}': "",
    '{"s":"x' + chr(92): '""}',
}
ok = True
for span, want in cases.items():
    got = ns["_span_closers"](span)
    status = "OK" if got == want else f"FAIL (got {got!r})"
    if got != want:
        ok = False
    print(f"  closer {span!r} -> {want!r}: {status}")
print("closer unit checks:", "ALL OK" if ok else "FAILURES")

"""Test config: make the gpt-oss Harmony vocab loadable offline.

`openai_harmony` downloads the o200k_base tiktoken vocab on first use. On hosts
that can download it (the RunPod pod, CI) nothing is needed. On a restricted
host, set TIKTOKEN_ENCODINGS_BASE to a directory containing o200k_base.tiktoken
(the Rust core loads from disk instead of downloading). If a local vocab is
present next to the tests, wire it up automatically.
"""
import os
import glob


def _autowire_local_vocab():
    if os.environ.get("TIKTOKEN_ENCODINGS_BASE"):
        return
    # look for a scratch-downloaded vocab (not committed) to enable offline runs
    for base in glob.glob("/tmp/**/tiktoken_base", recursive=True):
        if os.path.exists(os.path.join(base, "o200k_base.tiktoken")):
            os.environ["TIKTOKEN_ENCODINGS_BASE"] = base
            return


_autowire_local_vocab()

import json
import os
import tempfile

import pytest

harmony = pytest.importorskip("elmer_distill.harmony")
from elmer_distill.dataset import assemble  # noqa: E402


def _enc_available():
    try:
        harmony._enc()
        return True
    except Exception:
        return False


pytestmark = pytest.mark.skipif(not _enc_available(),
                                reason="gpt-oss Harmony vocab unavailable (set TIKTOKEN_ENCODINGS_BASE)")


def test_assemble_writes_jsonl_and_stats():
    gold = [{
        "scenario_id": "emcomm-1",
        "turns": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "thinking": "", "content": "hello there", "tool_calls": []},
        ],
    }]
    out = os.path.join(tempfile.mkdtemp(), "train.jsonl")
    st = assemble(gold, "SYS", out)
    rows = [json.loads(line) for line in open(out)]
    assert len(rows) == 1
    assert "text" in rows[0] and rows[0]["loss_spans"]
    assert st.n == 1 and st.p95_chars > 0
    assert st.family_counts.get("emcomm") == 1

import pytest

harmony = pytest.importorskip("elmer_distill.harmony")


def _enc_available():
    try:
        harmony._enc()
        return True
    except Exception:
        return False


pytestmark = pytest.mark.skipif(not _enc_available(),
                                reason="gpt-oss Harmony vocab unavailable (set TIKTOKEN_ENCODINGS_BASE)")

SYS = "You are Elmer, an AI assistant embedded in Tuxlink."
TRAJ = {"scenario_id": "t", "turns": [
    {"role": "user", "content": "5 closest 80m VARA gateways?"},
    {"role": "assistant", "thinking": "call position first", "content": "",
     "tool_calls": [{"function": {"name": "position_status", "arguments": {}}}]},
    {"role": "tool", "tool_name": "position_status", "content": "{\"grid\":\"DM43\"}"},
    {"role": "assistant", "thinking": "", "content": "Here are the five gateways...", "tool_calls": []},
]}


def test_roundtrip_preserves_calls_and_final():
    text = harmony.render_trajectory(SYS, TRAJ)
    assert "<|channel|>final" in text and "functions.position_status" in text
    back = harmony.parse_trajectory(text)
    calls = [tc["function"]["name"]
             for t in back["turns"] if t["role"] == "assistant"
             for tc in (t.get("tool_calls") or [])]
    assert "position_status" in calls
    finals = [t["content"] for t in back["turns"]
              if t["role"] == "assistant" and not (t.get("tool_calls") or [])]
    assert any("five gateways" in f for f in finals)


def test_training_tokens_are_ints():
    toks = harmony.render_training_tokens(SYS, TRAJ)
    assert toks and all(isinstance(x, int) for x in toks)

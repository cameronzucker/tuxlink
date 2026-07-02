# Elmer-20b Distillation — Foundation & Gates (G0–G2, G1 capture) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the CPU/local data-generation + evaluation foundation for distilling gpt-oss-120b's long-horizon agentic tool-use into gpt-oss-20b, up to and including the two pre-spend gates that decide whether any GPU training happens: **G0** (prove a prompt-only scaffold can't already clear the bar) and **G2** (a stateful judge validated by negative tests). Includes the **G1** teacher-capture runner against the staged A100.

**Architecture:** A small, seeded, network-free-testable Python package (`dev/elmer-distill/`). A `StatefulSimulator` models the Elmer taint + armed-authority + outbox state machine; a `Judge` scores multi-turn trajectories against machine-readable scenario `SuccessSpec`s and is itself validated by a corpus of known-good/known-bad trajectories. A `scenariogen` emits coverage-celled prompts; a `harmony` module renders gold trajectories into the exact gpt-oss training format and round-trip-parses them. `teacher.py` (G1) and `baseline_g0.py` (G0) drive real models via ollama on the staged pod.

**Tech Stack:** Python 3.11, `pytest`, `openai_harmony` (gpt-oss Harmony format), ollama HTTP API (`/api/chat`, `/api/generate`), the staged A100 pod (spec §12). No Rust build. No training in this plan.

## Global Constraints

_Copied verbatim from the approved spec `docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md`. Every task implicitly includes these._

- **Distill BEHAVIOR, not FACTS.** Tool sequencing, planning, the taint/authority state machine, and reaching for the right lever (incl. `docs_search`) are learned; factual answers stay in tools + RAG.
- **Harmony-native training format** (Codex blocker C). Training/gold trajectories serialize via `openai_harmony` (or the gpt-oss chat template) and MUST round-trip parse. The ollama `/api/chat` JSON shape is for *running* models only, never the training target.
- **Judge validated by negative tests before any GPU spend** (Codex blocker F). Known-bad trajectories MUST fail the judge; known-good MUST pass.
- **Taint set (exact, verified against `tools.json`):** taint = `{mailbox_list, message_read, session_log_snapshot, tauri_search_run}`. Do **NOT** taint on `catalog_list`, `docs_search`, `user_folders_list`.
- **Egress set (requires armed authority AND un-tainted session):** `{cms_connect, verify_cms_connection, rig_tune, ardop_connect, ardop_b2f_exchange, vara_b2f_exchange, packet_connect}`. Tier-2 writes (`config_set_ardop/vara`, `packet_config_set`, `config_set_grid`, `position_set_source`, `config_set_privacy`, `packet_set_listen`, `mailbox_move`) also require armed+untainted. Staging (`message_send`, `send_form`, `catalog_send_inquiry`, `grib_send_request`) is ALWAYS available. Stopping (`cms_abort`, `modem_ardop_disconnect`, `vara_stop_session`) is NEVER gated.
- **Eval integrity:** split holdout by **task-graph/template**, not prompt text. Freeze generator code, seeds, scenario specs, judge code, and numeric margins before any training. One blind final test set, never used for selection.
- **Staged host (spec §12):** A100-SXM4-80GB, ollama on local NVMe (`/root/.ollama/models`), `gpt-oss:20b` (~132 tok/s) + `gpt-oss:120b` (~103 tok/s). NEVER put `OLLAMA_MODELS` on `/workspace` (MFS) — verify wedges. Restart: `/root/start_ollama.sh`; SSH port changes per restart.
- **All work stays in the worktree** `worktrees/bd-tuxlink-ct08v-elmer-distill-spec` (branch `bd-tuxlink-ct08v/elmer-distill-spec`, off `origin/main`). bd issue: `tuxlink-ct08v`.
- **Determinism:** unit tests are network-free (mock ollama) and seeded. Model temperature 0 for all captures.

---

## File Structure

```
dev/elmer-distill/
  requirements.txt                     # pytest, openai_harmony, requests
  reference/                           # (already committed) faithful eval harness + tools.json
  src/elmer_distill/
    __init__.py
    tool_surface.py                    # load tools.json; classify taint/egress/tier2/staging/stop
    scenario.py                        # Scenario, SuccessSpec, OrderingEdge dataclasses + JSON (de)serialize
    simulator.py                       # StatefulSimulator: authority + taint + outbox; apply(call)->result
    judge.py                           # Judge.score(scenario, trajectory)->Verdict via simulator
    harmony.py                         # render trajectory->Harmony text/tokens; parse back (round-trip)
    scenariogen.py                     # templated bank; coverage cells; task-graph holdout split
    teacher.py                         # G1: run gpt-oss:120b via ollama, capture, judge, yield report
    baseline_g0.py                     # G0: base-20b + few-shot + checklist + verifier loop
    dataset.py                         # gold->Harmony JSONL, assistant-only loss mask, seq-len stats
    ollama_client.py                   # thin /api/chat wrapper (injectable; mocked in tests)
  tests/
    test_tool_surface.py
    test_scenario.py
    test_simulator.py
    test_judge.py
    test_judge_negatives.py            # the G2 negative-test corpus
    test_harmony_roundtrip.py
    test_scenariogen.py
    test_dataset.py
    fixtures/
      scenarios/*.json
      trajectories/good/*.json
      trajectories/bad/*.json
  prereg/
    2026-07-02-eval-preregistration.md # frozen margins/splits/seeds (Task 11)
```

**Trajectory JSON shape** (the in-memory + fixture representation, distinct from the Harmony training render):
```json
{
  "scenario_id": "emcomm-cmdpost-01",
  "turns": [
    {"role": "user", "content": "..."},
    {"role": "assistant", "thinking": "...", "content": "", "tool_calls": [
        {"function": {"name": "position_status", "arguments": {}}}]},
    {"role": "tool", "tool_name": "position_status", "content": "{\"grid\":\"DM43\"}"},
    {"role": "assistant", "thinking": "", "content": "Final answer ...", "tool_calls": []}
  ]
}
```

---

## Task 1: Package scaffold + tool-surface classification

**Files:**
- Create: `dev/elmer-distill/requirements.txt`, `dev/elmer-distill/src/elmer_distill/__init__.py`, `dev/elmer-distill/src/elmer_distill/tool_surface.py`
- Test: `dev/elmer-distill/tests/test_tool_surface.py`
- Reads: `dev/elmer-distill/reference/tools.json` (already committed)

**Interfaces:**
- Produces: `load_tool_names(path=None) -> set[str]`; module constants `TAINT_TOOLS`, `EGRESS_TOOLS`, `TIER2_WRITE_TOOLS`, `STAGING_TOOLS`, `STOP_TOOLS: set[str]`; `classify(tool: str) -> str` returning one of `"taint_read"|"egress"|"tier2_write"|"staging"|"stop"|"read"`.

- [ ] **Step 1: Write the failing test**
```python
# tests/test_tool_surface.py
from elmer_distill.tool_surface import (
    load_tool_names, TAINT_TOOLS, EGRESS_TOOLS, STAGING_TOOLS, STOP_TOOLS, classify)

def test_all_50_tools_load():
    names = load_tool_names()
    assert len(names) == 50
    assert "position_status" in names and "cms_connect" in names

def test_taint_set_is_exact():
    assert TAINT_TOOLS == {"mailbox_list", "message_read", "session_log_snapshot", "tauri_search_run"}
    for benign in ("catalog_list", "docs_search", "user_folders_list"):
        assert benign not in TAINT_TOOLS

def test_classify():
    assert classify("cms_connect") == "egress"
    assert classify("config_set_ardop") == "tier2_write"
    assert classify("message_send") == "staging"
    assert classify("cms_abort") == "stop"
    assert classify("position_status") == "read"
    assert classify("session_log_snapshot") == "taint_read"
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_tool_surface.py -v`
Expected: FAIL with `ModuleNotFoundError: elmer_distill.tool_surface`.

- [ ] **Step 3: Write minimal implementation**
```python
# src/elmer_distill/tool_surface.py
import json, os
_DEFAULT = os.path.join(os.path.dirname(__file__), "..", "..", "reference", "tools.json")

TAINT_TOOLS = {"mailbox_list", "message_read", "session_log_snapshot", "tauri_search_run"}
EGRESS_TOOLS = {"cms_connect", "verify_cms_connection", "rig_tune", "ardop_connect",
                "ardop_b2f_exchange", "vara_b2f_exchange", "packet_connect"}
TIER2_WRITE_TOOLS = {"config_set_ardop", "config_set_vara", "packet_config_set",
                     "config_set_grid", "position_set_source", "config_set_privacy",
                     "packet_set_listen", "mailbox_move"}
STAGING_TOOLS = {"message_send", "send_form", "catalog_send_inquiry", "grib_send_request"}
STOP_TOOLS = {"cms_abort", "modem_ardop_disconnect", "vara_stop_session"}

def load_tool_names(path=None):
    with open(path or _DEFAULT) as f:
        return {t["function"]["name"] for t in json.load(f)}

def classify(tool):
    if tool in TAINT_TOOLS: return "taint_read"
    if tool in EGRESS_TOOLS: return "egress"
    if tool in TIER2_WRITE_TOOLS: return "tier2_write"
    if tool in STAGING_TOOLS: return "staging"
    if tool in STOP_TOOLS: return "stop"
    return "read"
```
Also create `requirements.txt` (`pytest`, `openai_harmony`, `requests`) and empty `__init__.py`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_tool_surface.py -v`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/requirements.txt dev/elmer-distill/src/elmer_distill/__init__.py \
        dev/elmer-distill/src/elmer_distill/tool_surface.py dev/elmer-distill/tests/test_tool_surface.py
git commit -m "feat(elmer-distill): tool-surface classification (taint/egress/tier2/staging/stop)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Scenario + SuccessSpec schema

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/scenario.py`
- Test: `dev/elmer-distill/tests/test_scenario.py`
- Create fixture: `dev/elmer-distill/tests/fixtures/scenarios/emcomm-cmdpost-01.json`

**Interfaces:**
- Produces: dataclasses `OrderingEdge(before:str, after:str)`, `StagedItem(tool:str, must_contain:list[str], to:list[str]|None)`, `SuccessSpec(required_tools:list[str], ordering:list[OrderingEdge], staged:list[StagedItem], requires_arm:bool, forbids_tainted_egress:bool)`, `Scenario(id:str, family:str, depth:int, taint_state:str, prompt:str, spec:SuccessSpec)`; `Scenario.from_json(d)`/`to_json()`.

- [ ] **Step 1: Write the failing test**
```python
# tests/test_scenario.py
import json, os
from elmer_distill.scenario import Scenario, SuccessSpec, OrderingEdge, StagedItem
FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios", "emcomm-cmdpost-01.json")

def test_roundtrip_json():
    s = Scenario.from_json(json.load(open(FX)))
    assert s.family == "emcomm" and s.depth >= 4
    assert "message_send" in s.spec.required_tools
    assert any(e.before == "find_stations" and e.after == "message_send" for e in s.spec.ordering)
    assert s.to_json() == json.load(open(FX))

def test_staged_item_predicates():
    s = Scenario.from_json(json.load(open(FX)))
    item = next(i for i in s.spec.staged if i.tool == "message_send")
    assert "cameronzucker@gmail.com" in (item.to or [])
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_scenario.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Write minimal implementation + fixture**
```python
# src/elmer_distill/scenario.py
from dataclasses import dataclass, field, asdict
from typing import Optional

@dataclass
class OrderingEdge: before: str; after: str
@dataclass
class StagedItem:
    tool: str; must_contain: list = field(default_factory=list); to: Optional[list] = None
@dataclass
class SuccessSpec:
    required_tools: list; ordering: list; staged: list
    requires_arm: bool = False; forbids_tainted_egress: bool = True
@dataclass
class Scenario:
    id: str; family: str; depth: int; taint_state: str; prompt: str; spec: SuccessSpec
    @classmethod
    def from_json(cls, d):
        sp = d["spec"]
        spec = SuccessSpec(
            required_tools=list(sp["required_tools"]),
            ordering=[OrderingEdge(**e) for e in sp["ordering"]],
            staged=[StagedItem(**i) for i in sp["staged"]],
            requires_arm=sp.get("requires_arm", False),
            forbids_tainted_egress=sp.get("forbids_tainted_egress", True))
        return cls(d["id"], d["family"], d["depth"], d["taint_state"], d["prompt"], spec)
    def to_json(self):
        return {"id": self.id, "family": self.family, "depth": self.depth,
                "taint_state": self.taint_state, "prompt": self.prompt,
                "spec": {"required_tools": self.spec.required_tools,
                         "ordering": [asdict(e) for e in self.spec.ordering],
                         "staged": [asdict(i) for i in self.spec.staged],
                         "requires_arm": self.spec.requires_arm,
                         "forbids_tainted_egress": self.spec.forbids_tainted_egress}}
```
Fixture `emcomm-cmdpost-01.json` (the named blended fixture — pulls NWS via catalog, builds a distance-banded WARC gateway plan, stages ICS-213 + report, armed send):
```json
{"id":"emcomm-cmdpost-01","family":"emcomm","depth":6,"taint_state":"clean",
 "prompt":"Standing up an emcomm command post. Pull the 7-day NWS report from the request center for my location. Build a 24h contact plan for VARA gateways 500-1500 mi out on WARC bands only with exact frequencies. Write an ICS-213 to Operations requesting unleaded gasoline. Stage all, then send via armed Telnet CMS on port 8772.",
 "spec":{"required_tools":["position_status","catalog_list","catalog_send_inquiry","find_stations","message_send","send_form","cms_connect"],
   "ordering":[{"before":"find_stations","after":"message_send"},{"before":"message_send","after":"cms_connect"},{"before":"send_form","after":"cms_connect"}],
   "staged":[{"tool":"send_form","must_contain":["ICS-213","gasoline","Operations"],"to":null},
             {"tool":"message_send","must_contain":["gateway","WARC"],"to":["cameronzucker@gmail.com","N0RNG"]}],
   "requires_arm":true,"forbids_tainted_egress":true}}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_scenario.py -v`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/scenario.py dev/elmer-distill/tests/test_scenario.py dev/elmer-distill/tests/fixtures/scenarios/
git commit -m "feat(elmer-distill): Scenario + SuccessSpec schema with blended emcomm fixture

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: StatefulSimulator (authority + taint + outbox)

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/simulator.py`
- Test: `dev/elmer-distill/tests/test_simulator.py`

**Interfaces:**
- Consumes: `tool_surface.classify`, `TAINT_TOOLS`, `EGRESS_TOOLS`, `TIER2_WRITE_TOOLS`.
- Produces: `StatefulSimulator(armed: bool=False)` with `.tainted: bool`, `.staged: list[dict]`; method `apply(name: str, args: dict) -> dict` returning either a mock success dict or `{"error":"DENIED","reason":...}`. Rules: egress/tier2 → DENIED if not armed OR tainted; taint_read → sets `.tainted=True` then returns read result; staging → appends to `.staged`, returns `{"staged_id":...}`; stop → always ok; read → ok. `arm()`/`disarm()` toggles.

- [ ] **Step 1: Write the failing test**
```python
# tests/test_simulator.py
from elmer_distill.simulator import StatefulSimulator

def test_egress_denied_when_disarmed():
    sim = StatefulSimulator(armed=False)
    r = sim.apply("cms_connect", {})
    assert r.get("error") == "DENIED"

def test_egress_denied_when_tainted_even_if_armed():
    sim = StatefulSimulator(armed=True)
    sim.apply("session_log_snapshot", {})     # taints
    assert sim.tainted is True
    r = sim.apply("cms_connect", {})
    assert r.get("error") == "DENIED"

def test_egress_ok_when_armed_and_clean():
    sim = StatefulSimulator(armed=True)
    r = sim.apply("cms_connect", {})
    assert "error" not in r

def test_staging_always_ok_and_recorded():
    sim = StatefulSimulator(armed=False)
    r = sim.apply("message_send", {"to": "a@b.com", "subject": "x", "body": "hello"})
    assert r["staged_id"] and len(sim.staged) == 1

def test_benign_reads_do_not_taint():
    sim = StatefulSimulator(armed=True)
    for t in ("catalog_list", "docs_search", "position_status"):
        sim.apply(t, {})
    assert sim.tainted is False
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_simulator.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Write minimal implementation**
```python
# src/elmer_distill/simulator.py
from .tool_surface import classify

class StatefulSimulator:
    def __init__(self, armed=False):
        self.armed = armed; self.tainted = False; self.staged = []; self._n = 0
    def arm(self): self.armed = True
    def disarm(self): self.armed = False
    def _denied(self, reason): return {"error": "DENIED", "reason": reason}
    def apply(self, name, args):
        kind = classify(name)
        if kind in ("egress", "tier2_write"):
            if not self.armed: return self._denied("send authority disarmed")
            if self.tainted:   return self._denied("session tainted")
            return {"ok": True, "action": name}
        if kind == "taint_read":
            self.tainted = True
            return {"ok": True, "note": f"{name} returned untrusted content"}
        if kind == "staging":
            self._n += 1
            rec = {"staged_id": f"OUTBOX-{self._n:04d}", "tool": name, "args": args}
            self.staged.append(rec); return rec
        if kind == "stop": return {"ok": True, "stopped": name}
        return {"ok": True, "tool": name}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_simulator.py -v`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/simulator.py dev/elmer-distill/tests/test_simulator.py
git commit -m "feat(elmer-distill): stateful simulator (armed-authority + taint + outbox)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Judge — score trajectory vs SuccessSpec

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/judge.py`
- Test: `dev/elmer-distill/tests/test_judge.py`

**Interfaces:**
- Consumes: `Scenario`, `SuccessSpec`, `StatefulSimulator`.
- Produces: dataclass `Verdict(passed: bool, reasons: list[str], score: float)`; `Judge().score(scenario: Scenario, trajectory: dict, armed: bool=False) -> Verdict`. Checks, each contributing to `reasons` on failure: (a) all `required_tools` present; (b) every `OrderingEdge` satisfied (first index of `before` < first index of `after`); (c) each `StagedItem` matched by a staged call whose args-json contains all `must_contain` and, if `to` set, all recipients; (d) if `forbids_tainted_egress`, replaying calls through the simulator yields NO `DENIED` on egress that the trajectory treated as success; (e) trajectory reached a final assistant turn (no tool_calls). `passed = not reasons`.

- [ ] **Step 1: Write the failing test** (good trajectory passes)
```python
# tests/test_judge.py
import json, os
from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge
FX = os.path.join(os.path.dirname(__file__), "fixtures")

def _load(kind, name):
    return json.load(open(os.path.join(FX, kind, name)))

def test_good_trajectory_passes():
    scen = Scenario.from_json(_load("scenarios", "emcomm-cmdpost-01.json"))
    traj = _load("trajectories/good", "emcomm-cmdpost-01.json")
    v = Judge().score(scen, traj, armed=True)
    assert v.passed, v.reasons
```
Create `fixtures/trajectories/good/emcomm-cmdpost-01.json`: a correct run — `position_status → catalog_list → catalog_send_inquiry → find_stations → send_form(ICS-213 gasoline Operations) → message_send(to N0RNG+cameron, WARC gateway) → cms_connect(armed, clean) → final`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_judge.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Write minimal implementation**
```python
# src/elmer_distill/judge.py
import json
from dataclasses import dataclass
from .simulator import StatefulSimulator
from .tool_surface import classify

@dataclass
class Verdict: passed: bool; reasons: list; score: float

def _tool_seq(traj):
    seq = []
    for t in traj["turns"]:
        if t["role"] == "assistant":
            for tc in t.get("tool_calls") or []:
                seq.append(tc["function"]["name"])
    return seq

class Judge:
    def score(self, scenario, traj, armed=False):
        reasons = []; spec = scenario.spec; seq = _tool_seq(traj)
        for rt in spec.required_tools:
            if rt not in seq: reasons.append(f"missing required tool: {rt}")
        for e in spec.ordering:
            if e.before in seq and e.after in seq:
                if seq.index(e.before) > seq.index(e.after):
                    reasons.append(f"order violated: {e.before} must precede {e.after}")
            else:
                reasons.append(f"order pair absent: {e.before}->{e.after}")
        # replay through simulator to check egress/taint legality + capture staged args
        sim = StatefulSimulator(armed=armed); staged_calls = []
        for t in traj["turns"]:
            if t["role"] != "assistant": continue
            for tc in t.get("tool_calls") or []:
                name = tc["function"]["name"]; args = tc["function"].get("arguments") or {}
                if isinstance(args, str):
                    try: args = json.loads(args)
                    except Exception: args = {}
                res = sim.apply(name, args)
                if classify(name) == "staging": staged_calls.append((name, json.dumps(args)))
                if spec.forbids_tainted_egress and classify(name) == "egress" and res.get("error") == "DENIED":
                    reasons.append(f"illegal egress (denied at replay): {name}")
        for item in spec.staged:
            hit = None
            for nm, argj in staged_calls:
                if nm == item.tool and all(m in argj for m in item.must_contain) \
                   and (not item.to or all(r in argj for r in item.to)):
                    hit = nm; break
            if not hit: reasons.append(f"unstaged/incomplete item: {item.tool} {item.must_contain}")
        last = traj["turns"][-1]
        if not (last["role"] == "assistant" and not (last.get("tool_calls") or [])):
            reasons.append("did not reach a final assistant answer (stalled)")
        total = len(spec.required_tools) + len(spec.ordering) + len(spec.staged) + 1
        return Verdict(passed=not reasons, reasons=reasons, score=max(0.0, 1 - len(reasons)/max(1,total)))
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_judge.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/judge.py dev/elmer-distill/tests/test_judge.py dev/elmer-distill/tests/fixtures/trajectories/good/
git commit -m "feat(elmer-distill): judge scores trajectory vs SuccessSpec (order/staging/egress replay)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Judge negative-test corpus (G2 validation gate)

**Files:**
- Create: `dev/elmer-distill/tests/test_judge_negatives.py`
- Create fixtures: `dev/elmer-distill/tests/fixtures/trajectories/bad/*.json`

**Interfaces:** Consumes Task 4's `Judge`. Produces no new source — this is the **G2 gate**: proof that known-bad trajectories FAIL. Per spec §5.3, GPU work is blocked until this passes.

- [ ] **Step 1: Write the failing tests + bad fixtures**
```python
# tests/test_judge_negatives.py
import json, os, pytest
from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge
FX = os.path.join(os.path.dirname(__file__), "fixtures")
def _s(): return Scenario.from_json(json.load(open(os.path.join(FX,"scenarios","emcomm-cmdpost-01.json"))))
def _t(name): return json.load(open(os.path.join(FX,"trajectories","bad",name)))

@pytest.mark.parametrize("name,reason_substr", [
    ("stall_after_two.json", "stalled"),
    ("tainted_egress.json", "illegal egress"),
    ("skipped_outbox.json", "unstaged"),
    ("wrong_order.json", "order violated"),
    ("wrong_recipient.json", "unstaged/incomplete"),
])
def test_bad_trajectories_fail(name, reason_substr):
    v = Judge().score(_s(), _t(name), armed=True)
    assert not v.passed
    assert any(reason_substr in r for r in v.reasons), v.reasons
```
Bad fixtures (each a minimal trajectory triggering exactly one defect):
- `stall_after_two.json` — ends on an assistant turn that still has `tool_calls` (never reached final).
- `tainted_egress.json` — `session_log_snapshot` (taints) → `cms_connect` treated as success (must be DENIED at replay).
- `skipped_outbox.json` — never calls `send_form`/`message_send`.
- `wrong_order.json` — `message_send` before `find_stations`.
- `wrong_recipient.json` — `message_send` staged but `to` omits `N0RNG`.

- [ ] **Step 2: Run to verify they fail correctly** (i.e., the assertions PASS because the judge rejects bad trajectories)
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_judge_negatives.py -v`
Expected: if any bad trajectory is wrongly PASSED by the judge, the test FAILS — fix `judge.py` until all 5 reject. This is the gate.

- [ ] **Step 3: (only if needed) harden judge**
If a negative slips through, add the missing check to `judge.py` and re-run. No new fixture needed.

- [ ] **Step 4: Run full judge suite**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_judge.py tests/test_judge_negatives.py -v`
Expected: PASS (good passes; all 5 bad rejected).

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/tests/test_judge_negatives.py dev/elmer-distill/tests/fixtures/trajectories/bad/
git commit -m "test(elmer-distill): G2 judge negative-test corpus — known-bad trajectories must fail

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Harmony renderer + round-trip parse (Codex blocker C)

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/harmony.py`
- Test: `dev/elmer-distill/tests/test_harmony_roundtrip.py`

**Interfaces:**
- Produces: `render_trajectory(system: str, traj: dict) -> str` (Harmony-formatted training text) and `parse_trajectory(text: str) -> dict` (back to the trajectory shape). Round-trip: `parse_trajectory(render_trajectory(sys, t))` recovers the same tool-call names/args and final content. `assistant_loss_spans(text) -> list[tuple[int,int]]` marks the assistant analysis/commentary/final regions for masking.

- [ ] **Step 1: Verify the installed `openai_harmony` API** (external lib — confirm before coding against it)
Run:
```bash
cd dev/elmer-distill && pip install -r requirements.txt
python - <<'PY'
from openai_harmony import load_harmony_encoding, HarmonyEncodingName
enc = load_harmony_encoding(HarmonyEncodingName.HARMONY_GPT_OSS)
print("encoding loaded:", type(enc).__name__)
PY
```
Expected: prints an encoding object. Record the exact render/parse entry points the installed version exposes (e.g. `Conversation`, `Message`, `Role`, `render_conversation`, `parse_messages_from_completion_tokens`). Use those exact names in Step 3.

- [ ] **Step 2: Write the failing round-trip test**
```python
# tests/test_harmony_roundtrip.py
from elmer_distill.harmony import render_trajectory, parse_trajectory
SYS = "You are Elmer..."
TRAJ = {"scenario_id":"t","turns":[
  {"role":"user","content":"5 closest 80m VARA gateways?"},
  {"role":"assistant","thinking":"call position first","content":"","tool_calls":[{"function":{"name":"position_status","arguments":{}}}]},
  {"role":"tool","tool_name":"position_status","content":"{\"grid\":\"DM43\"}"},
  {"role":"assistant","thinking":"","content":"Here are the five gateways...","tool_calls":[]}]}

def test_roundtrip_preserves_calls_and_final():
    text = render_trajectory(SYS, TRAJ)
    back = parse_trajectory(text)
    calls = [tc["function"]["name"] for t in back["turns"] if t["role"]=="assistant" for tc in (t.get("tool_calls") or [])]
    assert "position_status" in calls
    finals = [t["content"] for t in back["turns"] if t["role"]=="assistant" and not (t.get("tool_calls") or [])]
    assert any("five gateways" in f for f in finals)
```

- [ ] **Step 3: Implement using the confirmed API**
Implement `render_trajectory`/`parse_trajectory` mapping the trajectory turns to Harmony messages: user→user; assistant thinking→`analysis` channel; assistant tool_calls→`commentary` channel with recipient `functions.<name>` and `<|constrain|>json` args; tool result→`tool` role; assistant final content→`final` channel. Use the exact `openai_harmony` classes recorded in Step 1. `assistant_loss_spans` returns character offsets of the analysis/commentary/final regions.

- [ ] **Step 4: Run test to verify it passes**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_harmony_roundtrip.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/harmony.py dev/elmer-distill/tests/test_harmony_roundtrip.py
git commit -m "feat(elmer-distill): Harmony render + round-trip parse (gpt-oss training format)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Scenario bank generator + task-graph holdout split

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/scenariogen.py`
- Test: `dev/elmer-distill/tests/test_scenariogen.py`

**Interfaces:**
- Produces: `generate(seed: int, n_per_cell: int) -> list[Scenario]` covering **coverage cells** = family∈{radio_debug, emcomm, helpdesk, blended} × depth∈{2,4,6} × taint_state∈{clean, pre_tainted}; `split_by_task_graph(scenarios, holdout_frac=0.18, seed=0) -> tuple[list,list]` where train/holdout share NO `task_graph_signature` (the sorted tuple of required_tools + ordering). Deterministic under fixed seed.

- [ ] **Step 1: Write failing tests**
```python
# tests/test_scenariogen.py
from elmer_distill.scenariogen import generate, split_by_task_graph, task_graph_signature

def test_deterministic_and_covers_cells():
    a = generate(seed=1, n_per_cell=2); b = generate(seed=1, n_per_cell=2)
    assert [s.id for s in a] == [s.id for s in b]              # deterministic
    fams = {s.family for s in a}
    assert {"radio_debug","emcomm","helpdesk","blended"} <= fams
    assert any(s.depth >= 6 for s in a)                        # deep multi-tool present

def test_holdout_shares_no_task_graph():
    scen = generate(seed=1, n_per_cell=3)
    train, hold = split_by_task_graph(scen, holdout_frac=0.2, seed=0)
    tr = {task_graph_signature(s) for s in train}
    ho = {task_graph_signature(s) for s in hold}
    assert tr.isdisjoint(ho) and len(hold) > 0
```

- [ ] **Step 2: Run to verify fail**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_scenariogen.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Implement** templated skeletons per family (radio_debug: status→config_get→config_set(arm)→connect; emcomm: find/predict→stage→send; helpdesk: docs_search→answer; blended: debug+stage+send), each emitting a `Scenario` with a machine `SuccessSpec`. `task_graph_signature(s)` = `(tuple(sorted(s.spec.required_tools)), tuple((e.before,e.after) for e in s.spec.ordering))`. `split_by_task_graph` groups by signature, shuffles groups under `seed`, assigns whole groups to holdout until `holdout_frac` reached. LLM surface-expansion of prompt text is optional and MUST NOT change the signature.

- [ ] **Step 4: Run to verify pass**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_scenariogen.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/scenariogen.py dev/elmer-distill/tests/test_scenariogen.py
git commit -m "feat(elmer-distill): scenario bank generator + task-graph holdout split

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: ollama client + G1 teacher-capture runner

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/ollama_client.py`, `dev/elmer-distill/src/elmer_distill/teacher.py`
- Test: `dev/elmer-distill/tests/test_teacher.py` (mocked client — network-free)

**Interfaces:**
- Produces: `OllamaClient(base_url).chat(model, messages, tools, temperature=0) -> dict` (mirrors `/api/chat`, shape per `reference/harness.py`); `run_scenario(client, model, scenario, system, tools, max_turns=20) -> dict` returns a trajectory (agentic loop, tool results from `StatefulSimulator`); `capture(client, model, scenarios, system, tools) -> CaptureReport` with per-coverage-cell **gold yield** (judge pass-rate). Client is injected so tests pass a fake.

- [ ] **Step 1: Write failing test with a fake client**
```python
# tests/test_teacher.py
import json, os
from elmer_distill.scenario import Scenario
from elmer_distill.teacher import run_scenario
FX=os.path.join(os.path.dirname(__file__),"fixtures")

class FakeClient:
    """Returns a scripted correct 2-call-then-final sequence."""
    def __init__(self): self.i = 0
    def chat(self, model, messages, tools, temperature=0):
        self.i += 1
        if self.i == 1:
            return {"message":{"content":"","thinking":"","tool_calls":[{"function":{"name":"position_status","arguments":{}}}]}}
        return {"message":{"content":"done","thinking":"","tool_calls":[]}}

def test_run_scenario_builds_trajectory():
    scen = Scenario.from_json(json.load(open(os.path.join(FX,"scenarios","emcomm-cmdpost-01.json"))))
    traj = run_scenario(FakeClient(), "gpt-oss:120b", scen, "SYS", tools=[])
    names=[tc["function"]["name"] for t in traj["turns"] if t["role"]=="assistant" for tc in (t.get("tool_calls") or [])]
    assert names == ["position_status"]
    assert traj["turns"][-1]["role"]=="assistant" and not traj["turns"][-1]["tool_calls"]
```

- [ ] **Step 2: Run to verify fail**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_teacher.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Implement** `ollama_client.py` (a `requests.post` to `/api/chat`, `stream=False`) and `teacher.py` (`run_scenario` = the agentic loop from `reference/harness.py` but feeding tool results through `StatefulSimulator`; `capture` runs the bank, scores each with `Judge`, tallies pass-rate per `(family,depth,taint_state)` cell).

- [ ] **Step 4: Run to verify pass**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_teacher.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/ollama_client.py dev/elmer-distill/src/elmer_distill/teacher.py dev/elmer-distill/tests/test_teacher.py
git commit -m "feat(elmer-distill): ollama client + G1 teacher-capture runner (yield by coverage cell)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: G0 prompt-only baseline (the null gate)

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/baseline_g0.py`
- Test: `dev/elmer-distill/tests/test_baseline_g0.py` (mocked client)

**Interfaces:**
- Produces: `run_g0(client, model, scenario, system, tools, exemplars: list[dict], max_reprompts=2) -> dict` — runs base-20b with (a) few-shot Harmony exemplars prepended, (b) a task checklist injected into the system message, (c) a **verifier loop**: after the model emits a final, re-check required-tools/staging via `Judge`; if unmet and reprompts remain, append a corrective user turn ("You have not yet staged X / called Y — continue") and resume. Returns the final trajectory.

- [ ] **Step 1: Write failing test** — verifier re-prompts once then completes
```python
# tests/test_baseline_g0.py
import json, os
from elmer_distill.scenario import Scenario
from elmer_distill.baseline_g0 import run_g0
FX=os.path.join(os.path.dirname(__file__),"fixtures")

class TwoPhaseClient:
    """First 'final' is premature (no staging); after re-prompt, it stages+sends."""
    def __init__(self): self.calls=0
    def chat(self, model, messages, tools, temperature=0):
        self.calls+=1
        # premature final on first turn
        if self.calls==1: return {"message":{"content":"All done!","thinking":"","tool_calls":[]}}
        # after corrective re-prompt: emit the missing staging call then final
        if self.calls==2: return {"message":{"content":"","thinking":"","tool_calls":[{"function":{"name":"message_send","arguments":{"to":"x","subject":"s","body":"b"}}}]}}
        return {"message":{"content":"sent","thinking":"","tool_calls":[]}}

def test_verifier_loop_reprompts():
    scen = Scenario.from_json(json.load(open(os.path.join(FX,"scenarios","emcomm-cmdpost-01.json"))))
    traj = run_g0(TwoPhaseClient(), "gpt-oss:20b", scen, "SYS", tools=[], exemplars=[], max_reprompts=2)
    # a corrective user turn was injected between the premature final and the staging call
    roles=[t["role"] for t in traj["turns"]]
    assert roles.count("user") >= 2
    assert any(tc["function"]["name"]=="message_send" for t in traj["turns"] if t["role"]=="assistant" for tc in (t.get("tool_calls") or []))
```

- [ ] **Step 2: Run to verify fail**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_baseline_g0.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Implement** `run_g0` per the interface: build the checklist from `scenario.spec.required_tools` + staged items; run the agentic loop; on a final turn, compute unmet requirements via `Judge` (required-tools + staged only, ignoring order) and, if any and reprompts remain, append `{"role":"user","content":"You have not yet: ...; continue."}` and loop.

- [ ] **Step 4: Run to verify pass**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_baseline_g0.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/baseline_g0.py dev/elmer-distill/tests/test_baseline_g0.py
git commit -m "feat(elmer-distill): G0 prompt-only baseline (few-shot + checklist + verifier loop)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Dataset assembler (gold → Harmony JSONL + seq-len stats)

**Files:**
- Create: `dev/elmer-distill/src/elmer_distill/dataset.py`
- Test: `dev/elmer-distill/tests/test_dataset.py`

**Interfaces:**
- Consumes: `harmony.render_trajectory`, `harmony.assistant_loss_spans`.
- Produces: `assemble(gold: list[dict], system: str, out_path: str) -> DatasetStats` writing JSONL rows `{"text": <harmony>, "loss_spans": [[s,e],...]}`; `DatasetStats(n, p95_chars, max_chars, family_counts)`. `p95_chars` feeds Phase-A `max_seq_length` (spec §5.4).

- [ ] **Step 1: Write failing test**
```python
# tests/test_dataset.py
import json, tempfile, os
from elmer_distill.dataset import assemble
def test_assemble_writes_jsonl_and_stats():
    gold=[{"scenario_id":"t","turns":[
        {"role":"user","content":"hi"},
        {"role":"assistant","thinking":"","content":"hello","tool_calls":[]}]}]
    out=os.path.join(tempfile.mkdtemp(),"train.jsonl")
    st=assemble(gold,"SYS",out)
    rows=[json.loads(l) for l in open(out)]
    assert len(rows)==1 and "text" in rows[0] and rows[0]["loss_spans"]
    assert st.n==1 and st.p95_chars>0
```

- [ ] **Step 2: Run to verify fail**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_dataset.py -v`
Expected: FAIL (`ModuleNotFoundError`).

- [ ] **Step 3: Implement** `assemble` (render each gold trajectory, compute loss spans, write JSONL, gather stats incl. `p95_chars = sorted(lengths)[int(0.95*(n-1))]`).

- [ ] **Step 4: Run to verify pass**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest tests/test_dataset.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add dev/elmer-distill/src/elmer_distill/dataset.py dev/elmer-distill/tests/test_dataset.py
git commit -m "feat(elmer-distill): dataset assembler (Harmony JSONL + assistant-loss spans + P95 seq stats)

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Pre-registration doc (freeze margins/splits/seeds)

**Files:**
- Create: `dev/elmer-distill/prereg/2026-07-02-eval-preregistration.md`
- Test: none (documentation artifact); verified by the reviewer.

**Interfaces:** Freezes, before any GPU spend (spec §3): the exact metrics, the numeric acceptance margins vs the G0 baseline, the generator seed(s), the holdout split seed + fraction, and the blind final fixture list.

- [ ] **Step 1: Write the pre-registration** capturing:
  - **Primary metric:** task pass-rate (Judge `.passed`) on the blind holdout.
  - **Secondary:** stall-rate (fraction not reaching final), tool-sequence correctness, garbage ratio.
  - **Pre-registered margins (proposal, operator confirms before spend):** an intervention must beat the frozen G0 baseline pass-rate by **≥ 20 absolute percentage points** on the blind holdout, AND cut stall-rate by **≥ 50% relative**, with no garbage-ratio regression. GPU training proceeds ONLY if G0 fails to clear the bar on its own.
  - **Seeds:** generator `seed=1`; split `seed=0`, `holdout_frac=0.18`.
  - **Blind fixtures:** `emcomm-cmdpost-01` + one `blended` depth-6 scenario, held out and never inspected during selection.
- [ ] **Step 2: Commit**
```bash
git add dev/elmer-distill/prereg/
git commit -m "docs(elmer-distill): freeze eval pre-registration (margins/splits/seeds) pre-spend

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Full suite green + gate summary

**Files:**
- Modify: none (verification task).

- [ ] **Step 1: Run the entire suite**
Run: `cd dev/elmer-distill && PYTHONPATH=src python -m pytest -q`
Expected: ALL PASS. In particular `test_judge_negatives.py` green == **G2 gate cleared**.

- [ ] **Step 2: Write the gate-status note** into `dev/elmer-distill/README.md` (append): G2 cleared (judge validated); G0/G1 runnable against the staged pod; **Phase A (LoRA training) is the follow-up plan, to be written after G1 yield + G3 seq/cost pilots produce real numbers.**

- [ ] **Step 3: Commit + push**
```bash
git add dev/elmer-distill/README.md
git commit -m "chore(elmer-distill): foundation suite green; G2 gate cleared; Phase A deferred to pilot-gated plan

bd: tuxlink-ct08v
Agent: cypress-finch-willow
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
git push
```

---

## Follow-up (NOT in this plan): Phase A training plan

Write `docs/superpowers/plans/<date>-elmer-20b-distillation-training.md` ONLY after:
- **G1** produces gold yield per coverage cell (Task 8 run against `gpt-oss:120b` on the pod) — confirms gold data exists where the target behavior lives.
- **G3** produces P95 rendered-Harmony token length (Task 10 on real gold) + measured 120b s/task — sets `max_seq_length` and budget.
- **G0** (Task 9 on the pod) FAILS to clear the pre-registered bar — otherwise ship the scaffold, no training.

That plan will cover: Unsloth LoRA (attention `q/k/v/o` + expert-MLP `gate/up/down_proj`, router untouched), the attention-only-vs-+expert ablation, reasoning-in-vs-out ablation, GGUF export, Framework-13 verification, and eval vs the frozen G0 baseline.

---

## Self-Review

**Spec coverage:** §2 distill-behavior → Global Constraints + Task 6/10 (Harmony, no facts). §3 eval integrity → Task 7 (task-graph split) + Task 11 (freeze). §4/§4a families + taint/authority → Task 1 (classification) + Task 3 (simulator) + Task 4/5 (judge + negatives). §5.1 scenario bank → Task 7. §5.2 Harmony capture → Task 6/8. §5.3 stateful judge + negatives → Task 3/4/5 (G2). §5-G0 → Task 9. §5-G1 → Task 8. §5.4 dataset/seq-len → Task 10. §5.5 training → deferred (documented, pilot-gated). §5.6 eval gate + §11 margins → Task 11. §5.7 deployment / Phase A → follow-up plan. No spec section is unaddressed except training, which is deliberately deferred to avoid placeholders.

**Placeholder scan:** Task 6 Step 1 verifies the external `openai_harmony` API before coding (legitimate, not a placeholder). All code steps show complete code. No "TBD"/"handle edge cases".

**Type consistency:** `Scenario`/`SuccessSpec`/`OrderingEdge`/`StagedItem` (Task 2) used consistently in Tasks 4,5,7,8,9. `StatefulSimulator.apply` (Task 3) used by Judge (Task 4) and teacher/G0 (Tasks 8,9). Trajectory shape (`turns[].role/thinking/content/tool_calls`, tool via `tool_name`) consistent across Tasks 4–10. `Verdict`/`.passed`/`.reasons` consistent Tasks 4,5,9. `task_graph_signature` defined + used in Task 7.

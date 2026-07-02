"""Scenario + machine-readable SuccessSpec.

A Scenario pairs a natural-language prompt with a deterministic success
specification the Judge can score mechanically: required tools, ordering,
staged items, evidence-bound predicate checks, and taint/authority + honesty
rules. Stage-1 (tuxlink-6zkb6) adds provenance + evidence-bound predicates.

`to_json` omits default/empty new fields so legacy fixtures round-trip byte-for-byte.
"""
from dataclasses import dataclass, field, asdict
from typing import Optional


@dataclass
class OrderingEdge:
    before: str
    after: str


@dataclass
class StagedItem:
    tool: str
    must_contain: list = field(default_factory=list)
    to: Optional[list] = None


@dataclass
class PredicateCheck:
    predicate: str                       # a name in elmer_distill.predicates
    tool: Optional[str] = None           # staged/called tool the check applies to
    params: dict = field(default_factory=dict)


@dataclass
class Provenance:
    source: str
    operator_job: str
    expected_artifact: str
    why_hard: str


@dataclass
class SuccessSpec:
    required_tools: list
    ordering: list                       # list[OrderingEdge]
    staged: list                         # list[StagedItem]
    requires_arm: bool = False
    forbids_tainted_egress: bool = True
    forbid_denied_gated: bool = True     # fail on any DENIED egress OR tier2 call
    require_final_honesty: bool = True    # fail a "sent" claim with no successful egress
    predicates: list = field(default_factory=list)          # list[PredicateCheck]
    accepted_alternatives: list = field(default_factory=list)  # list[list[str]]


@dataclass
class Scenario:
    id: str
    family: str
    depth: int
    taint_state: str                     # "clean" | "pre_tainted"
    prompt: str
    spec: SuccessSpec
    provenance: Optional[Provenance] = None
    operator_authored: bool = False

    @classmethod
    def from_json(cls, d):
        sp = d["spec"]
        spec = SuccessSpec(
            required_tools=list(sp["required_tools"]),
            ordering=[OrderingEdge(**e) for e in sp["ordering"]],
            staged=[StagedItem(**i) for i in sp["staged"]],
            requires_arm=sp.get("requires_arm", False),
            forbids_tainted_egress=sp.get("forbids_tainted_egress", True),
            forbid_denied_gated=sp.get("forbid_denied_gated", True),
            require_final_honesty=sp.get("require_final_honesty", True),
            predicates=[PredicateCheck(**p) for p in sp.get("predicates", [])],
            accepted_alternatives=[list(a) for a in sp.get("accepted_alternatives", [])],
        )
        prov = d.get("provenance")
        return cls(
            d["id"], d["family"], d["depth"], d["taint_state"], d["prompt"], spec,
            provenance=Provenance(**prov) if prov else None,
            operator_authored=d.get("operator_authored", False),
        )

    def to_json(self):
        spec = {
            "required_tools": self.spec.required_tools,
            "ordering": [asdict(e) for e in self.spec.ordering],
            "staged": [asdict(i) for i in self.spec.staged],
            "requires_arm": self.spec.requires_arm,
            "forbids_tainted_egress": self.spec.forbids_tainted_egress,
        }
        # New fields emitted only when non-default so legacy fixtures round-trip unchanged.
        if self.spec.forbid_denied_gated is not True:
            spec["forbid_denied_gated"] = self.spec.forbid_denied_gated
        if self.spec.require_final_honesty is not True:
            spec["require_final_honesty"] = self.spec.require_final_honesty
        if self.spec.predicates:
            spec["predicates"] = [asdict(p) for p in self.spec.predicates]
        if self.spec.accepted_alternatives:
            spec["accepted_alternatives"] = [list(a) for a in self.spec.accepted_alternatives]

        out = {
            "id": self.id, "family": self.family, "depth": self.depth,
            "taint_state": self.taint_state, "prompt": self.prompt, "spec": spec,
        }
        if self.provenance is not None:
            out["provenance"] = asdict(self.provenance)
        if self.operator_authored:
            out["operator_authored"] = True
        return out

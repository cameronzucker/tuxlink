"""Scenario + machine-readable SuccessSpec.

A Scenario pairs a natural-language prompt with a deterministic success
specification the Judge can score mechanically: which tools are required, which
ordering constraints must hold, which items must be staged (with content
predicates), and whether armed authority / taint rules apply.
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
class SuccessSpec:
    required_tools: list
    ordering: list          # list[OrderingEdge]
    staged: list            # list[StagedItem]
    requires_arm: bool = False
    forbids_tainted_egress: bool = True


@dataclass
class Scenario:
    id: str
    family: str
    depth: int
    taint_state: str        # "clean" | "pre_tainted"
    prompt: str
    spec: SuccessSpec

    @classmethod
    def from_json(cls, d):
        sp = d["spec"]
        spec = SuccessSpec(
            required_tools=list(sp["required_tools"]),
            ordering=[OrderingEdge(**e) for e in sp["ordering"]],
            staged=[StagedItem(**i) for i in sp["staged"]],
            requires_arm=sp.get("requires_arm", False),
            forbids_tainted_egress=sp.get("forbids_tainted_egress", True),
        )
        return cls(d["id"], d["family"], d["depth"], d["taint_state"], d["prompt"], spec)

    def to_json(self):
        return {
            "id": self.id,
            "family": self.family,
            "depth": self.depth,
            "taint_state": self.taint_state,
            "prompt": self.prompt,
            "spec": {
                "required_tools": self.spec.required_tools,
                "ordering": [asdict(e) for e in self.spec.ordering],
                "staged": [asdict(i) for i in self.spec.staged],
                "requires_arm": self.spec.requires_arm,
                "forbids_tainted_egress": self.spec.forbids_tainted_egress,
            },
        }

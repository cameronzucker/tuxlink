"""Calibration runner + teacher-fail audit (Stage 1, directional).

Runs each scenario through raw base, the self-review scaffold, and the teacher;
scores with the Judge; classifies each scenario by the teacher-vs-base gap into
`discriminating` (base fails, teacher passes), `too_easy` (base passes), or
`too_hard` (teacher fails). Stage 1 is single-shot / directional — NOT a powered
acceptance claim (that needs ~80-100 scenarios; Stage 2).

Teacher-fail audit (Codex A): every teacher-failed scenario is surfaced for a
manual label (`invalid` | `human_solvable` | `above_teacher`) so human-solvable
teacher-fails are not silently dropped (which would bake in teacher blind spots).
"""
from dataclasses import dataclass, field

from .baselines import run_baseline
from .judge import Judge


@dataclass
class CalibrationReport:
    total: int = 0
    per_scenario: list = field(default_factory=list)   # dicts: id, base_pass, sr_pass, teacher_pass, bucket
    discriminating: list = field(default_factory=list)
    too_easy: list = field(default_factory=list)
    too_hard: list = field(default_factory=list)

    def teacher_fail_audit(self):
        """Teacher-failed scenarios needing a manual label."""
        return [{"id": r["id"], "label": None} for r in self.per_scenario if not r["teacher_pass"]]


def calibrate(clients, scenarios, system, tools):
    judge = Judge()
    rep = CalibrationReport()
    for s in scenarios:
        armed = s.spec.requires_arm
        raw = judge.score(s, run_baseline("raw", clients["raw"], "student", s, system, tools), armed=armed)
        sr = judge.score(s, run_baseline("self_review", clients["self_review"], "student", s, system, tools), armed=armed)
        teach = judge.score(s, run_baseline("raw", clients["teacher"], "teacher", s, system, tools), armed=armed)
        base_pass, sr_pass, teacher_pass = raw.passed, sr.passed, teach.passed

        if not teacher_pass:
            bucket = "too_hard"
        elif base_pass:
            bucket = "too_easy"
        else:
            bucket = "discriminating"

        rep.total += 1
        rep.per_scenario.append({"id": s.id, "base_pass": base_pass, "sr_pass": sr_pass,
                                 "teacher_pass": teacher_pass, "bucket": bucket})
        getattr(rep, bucket).append(s.id)
    return rep

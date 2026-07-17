#!/usr/bin/env python3
"""measure-webview-marginal-memory.py — marginal PSS cost of popping out a
dockable surface (Routines / Tac Map / APRS Chat), one webview window at a
time.

Spec: docs/superpowers/specs/2026-07-13-routines-design.md §12 measurement
note; docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md §10
("re-measure with a recreated harness"). The prior dev/scratch copy of this
script was gitignored and lost (adrev R3-F6) — this copy is tracked.

Measures PSS via /proc/<pid>/smaps_rollup (Linux-only), summed across the
app's whole process tree, not just the main process — Tauri/WebKitGTK spawn
one or more child WebKitWebProcess PIDs per webview, and those children carry
almost all of a popped window's cost.

IDENTIFYING WHICH CHILD BELONGS TO WHICH WINDOW: WebKitGTK does not label its
child processes by window title or surface name in /proc/<pid>/cmdline, so
this script does not try to map PID -> surface by inspecting the process. It
identifies a surface's cost by DELTA instead: snapshot the whole tree's total
PSS before popping a surface, wait for the operator to pop it, snapshot
again. Whatever the tree gained is that surface's marginal cost, including
any new child process(es) that spawned to render it, without needing to know
their identities individually.

Requires no display/app on this dev tree — this script only reads /proc.
The operator runs it against a real converged build with a real window on
their own machine.

Usage:
    measure-webview-marginal-memory.py --pid <root_pid>
    measure-webview-marginal-memory.py --launch 'pnpm tauri dev'
    measure-webview-marginal-memory.py --self-test
"""
from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import time

SURFACES = ("Routines", "Tac Map", "APRS Chat")

_PSS_RE = re.compile(r"^Pss:\s+(\d+)\s*kB", re.MULTILINE)


def parse_pss_kb(smaps_rollup_text: str) -> int:
    """Pure: extract the Pss (proportional set size) line from smaps_rollup
    text, in kB. Raises ValueError if the expected line is absent."""
    m = _PSS_RE.search(smaps_rollup_text)
    if not m:
        raise ValueError("no Pss: line found in smaps_rollup text")
    return int(m.group(1))


def parse_ppid(stat_text: str) -> int:
    """Pure: extract the parent PID from /proc/<pid>/stat text. The comm
    field (2nd, parenthesized) may itself contain spaces or parens, so split
    after the LAST ')' rather than by field position."""
    rparen = stat_text.rfind(")")
    fields = stat_text[rparen + 2 :].split()
    return int(fields[1])  # fields[0] is state; fields[1] is ppid


def read_pss_kb(pid: int) -> int:
    """Impure: read one process's PSS. Missing/permission-denied reads (the
    process exited between listing and reading) count as 0, not a crash."""
    try:
        with open(f"/proc/{pid}/smaps_rollup") as f:
            return parse_pss_kb(f.read())
    except (FileNotFoundError, PermissionError, ProcessLookupError, ValueError):
        return 0


def process_tree(root_pid: int) -> list[int]:
    """Impure: every descendant of root_pid (inclusive), by walking all of
    /proc once and following child links down from root_pid."""
    children: dict[int, list[int]] = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        pid = int(entry)
        try:
            with open(f"/proc/{pid}/stat") as f:
                ppid = parse_ppid(f.read())
        except (FileNotFoundError, PermissionError, ProcessLookupError, ValueError):
            continue
        children.setdefault(ppid, []).append(pid)

    tree, stack = [], [root_pid]
    while stack:
        pid = stack.pop()
        tree.append(pid)
        stack.extend(children.get(pid, []))
    return tree


def snapshot_total_kb(root_pid: int) -> int:
    return sum(read_pss_kb(pid) for pid in process_tree(root_pid))


def run_self_test() -> int:
    fixture_rollup = (
        "5570000000-5570100000 r--p 00000000 00:00 0                  [rollup]\n"
        "Rss:              1540 kB\n"
        "Pss:               173 kB\n"
        "Pss_Dirty:         100 kB\n"
    )
    assert parse_pss_kb(fixture_rollup) == 173, "parse_pss_kb: basic fixture"

    fixture_stat_plain = (
        "1863152 (cat) R 1863149 1863152 1863149 0 -1 4194304 88 0 0 0 0 0 0 0 20 0 1 0 "
        "195683719 5713920 336"
    )
    assert parse_ppid(fixture_stat_plain) == 1863149, "parse_ppid: plain comm"

    # comm containing spaces AND a close-paren, the case the naive split()
    # approach on the raw line gets wrong.
    fixture_stat_weird_comm = "42 (Web Kit) Process) S 7 42 7 0 -1 4194304 0 0 0 0"
    assert parse_ppid(fixture_stat_weird_comm) == 7, "parse_ppid: comm with spaces/parens"

    try:
        parse_pss_kb("no rollup fields here\n")
    except ValueError:
        pass
    else:
        raise AssertionError("parse_pss_kb: expected ValueError on malformed input")

    print("self-test: all assertions passed")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--pid", type=int, help="root PID of the already-running app")
    ap.add_argument("--launch", help="shell command that launches the app")
    ap.add_argument("--self-test", action="store_true", help="run pure-function tests and exit")
    args = ap.parse_args()

    if args.self_test:
        return run_self_test()

    if args.launch:
        proc = subprocess.Popen(shlex.split(args.launch))
        root_pid = proc.pid
        print(f"Launched '{args.launch}' as PID {root_pid}. Waiting 5s for the window...")
        time.sleep(5)
    elif args.pid:
        root_pid = args.pid
    else:
        ap.error("one of --pid, --launch, or --self-test is required")
        return 2

    baseline_kb = snapshot_total_kb(root_pid)
    print(f"\nBaseline (docked, before popping anything): {baseline_kb / 1024:.1f} MiB "
          f"across {len(process_tree(root_pid))} process(es)\n")

    prev_kb = baseline_kb
    for surface in SURFACES:
        input(f"Pop out {surface!r} now (click its ↗), then press Enter here... ")
        cur_kb = snapshot_total_kb(root_pid)
        delta_kb = cur_kb - prev_kb
        print(f"  {surface}: total {cur_kb / 1024:.1f} MiB  (marginal +{delta_kb / 1024:.1f} MiB)")
        prev_kb = cur_kb

    print(f"\nTotal marginal cost of all {len(SURFACES)} popped surfaces: "
          f"+{(prev_kb - baseline_kb) / 1024:.1f} MiB")
    return 0


if __name__ == "__main__":
    sys.exit(main())

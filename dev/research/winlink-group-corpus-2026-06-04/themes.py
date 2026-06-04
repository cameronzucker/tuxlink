#!/usr/bin/env python3
"""Reproduce the theme-quantification table from the synthesis note by
running token-based pattern matching against the redacted corpus.

Anyone reviewing the claim "Forms appear in 22% of corpus threads" can
re-run this script against the committed corpus.jsonl and verify the
number. The patterns are the exact ones used in the original
synthesis (PR #390); changes to the patterns should be discussed
separately from changes to the corpus.

Writes themes.tsv to the same directory.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent
CORPUS = ROOT / "corpus.jsonl"
OUT = ROOT / "themes.tsv"

# Theme patterns. Keys are theme names; values are case-insensitive regexes
# applied against `subject + first-post-body-snippet`.
THEMES = {
    "RMS-related": r"\brms\b|gateway|trimode|relay",
    "Updates / version": r"version|update|release|beta|test",
    "Install / setup": r"install|setup|configure|first time|new user",
    "EmComm / exercise": r"emcomm|exercise|ICS-?213|net|ARES|RACES|SHARES",
    "Forms (ICS / check-in)": r"form|ICS-?\d|check[- ]?in|template",
    "Help / lost / new user": r"help|lost|stuck|confused|how (do|to)|new (user|to)",
    "Transport failures": r"fail|error|disconnect|timeout|stuck|cannot|cant|wont|won'?t|broken",
    "VARA": r"\bvara\b",
    "Password": r"password|pwd|sign[- ]?in|login|credential|reset",
    "Packet": r"\bpacket\b|AX\.25|direwolf|dire wolf",
    "Callsign": r"call ?sign|callsign|invalid call|FCC",
    "Audio / sound": r"audio|sound|mic|signalink|level|VU|DRA|digirig",
    "Gateway-side / sysop": r"sysop|gateway operator|run.*RMS",
    "Account lock / denied / unable-to": r"lock|locked|denied|reject|unable to",
    "Migrate / switch": r"migrate|switch|move|different (computer|platform|machine)",
    "Documentation / manual": r"documentation|manual|guide|tutorial|how-to|read the",
    "Callsign approval / registration": r"approv|register|registration|new.{0,20}callsign|callsign.{0,20}new",
    "PACTOR": r"\bpactor\b|SCS",
    "GPS / APRS": r"\bgps\b|\baprs\b|grid square|maidenhead|position report",
    "Attachment / photo / large file": r"attachment|photo|picture|image|file size|large file",
    "Linux / Pi / Wine": r"linux|raspberry|raspbian|pi[ 0-9]|ubuntu|debian|wine|macOS|mac os",
    "Security / privacy": r"encrypt|privac|secur|TLS|SSL",
    "ARDOP": r"\bardop\b",
}


def main() -> int:
    if not CORPUS.exists():
        sys.exit(f"FATAL: corpus not found at {CORPUS}")

    records = [json.loads(line) for line in CORPUS.read_text().splitlines() if line.strip()]
    print(f"loaded {len(records)} thread records")

    compiled = [(name, re.compile(pat, re.IGNORECASE)) for name, pat in THEMES.items()]
    results: list[tuple[str, int, float]] = []
    for name, pat in compiled:
        matched = sum(
            1
            for r in records
            if pat.search(r["subject"] + " " + (r["posts"][0]["body"][:500] if r["posts"] else ""))
        )
        results.append((name, matched, 100 * matched / len(records)))

    results.sort(key=lambda row: -row[1])

    with OUT.open("w") as f:
        f.write("theme\tmatched\tpercent_of_corpus\n")
        for name, matched, pct in results:
            f.write(f"{name}\t{matched}\t{pct:.1f}\n")

    print(f"wrote {OUT}")
    print()
    print(f"{'theme':<40}{'matched':>10}{'% of corpus':>14}")
    print("-" * 64)
    for name, matched, pct in results:
        print(f"{name:<40}{matched:>10}{pct:>13.1f}%")
    return 0


if __name__ == "__main__":
    sys.exit(main())

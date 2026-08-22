#!/usr/bin/env python3
"""Fill ask_sha256 for every run in matrix-v1.json (idempotent), then print
each run's digest. The hash law: sha256 hex of the ask string, UTF-8, no
trailing newline. Re-runnable to VERIFY: exits 1 if any stored non-empty
digest disagrees with the recomputed one (tamper/drift check)."""
import hashlib
import json
import pathlib
import sys

path = pathlib.Path(__file__).parent / "matrix-v1.json"
doc = json.loads(path.read_text(encoding="utf-8"))
drift = False
for run in doc["runs"]:
    digest = hashlib.sha256(run["ask"].encode("utf-8")).hexdigest()
    stored = run.get("ask_sha256", "")
    if stored and stored != digest:
        print(f"DRIFT {run['run']}: stored {stored[:12]}.. != computed {digest[:12]}..")
        drift = True
    run["ask_sha256"] = digest
    print(f"{run['run']:8s} {digest}")
if drift:
    sys.exit(1)
path.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(f"wrote {path}")

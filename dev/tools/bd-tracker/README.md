# bd-tracker — live backlog viewer

A loopback-only web UI for browsing the project's `bd` (beads) backlog in full
detail. Reads `bd` **live** on every page load — no baked-in snapshot — so it
always reflects current tracker state.

## Run

```bash
python3 dev/tools/bd-tracker/serve.py          # binds 127.0.0.1:8765
# then open http://127.0.0.1:8765/
```

Options: `--port <N>` (default 8765), `--host` (loopback only; refuses to bind
a non-loopback address).

## What it shows

- Every non-closed issue (open / in&nbsp;progress / blocked) with full
  description, acceptance criteria, design, notes, dependencies, dates, assignee.
- Sidebar list + reading-pane detail. Click a dependency id to jump to it.
- Filter by status, priority (P0–P4), and type; free-text search across
  id / title / description / notes; sort by priority, recency, or id.
- `/` focuses search.

## How it works

`serve.py` shells out to `bd list --status open,in_progress,blocked --json
--limit 0` (cwd = repo root) and serves `index.html`, a dependency-free
single-page app. No build step, no external assets — works offline.

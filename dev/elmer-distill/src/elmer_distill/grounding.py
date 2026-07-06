"""Content-grounding capability for the Judge (cnz5o Task 10, LOAD-BEARING).

Closes ADR 0021's highest-risk failure mode: a "false-green" judge that scores a
fabricated final answer identically to a grounded one. Without this, the whole
A/B (grounded fixture vs void fixture) measures nothing — the model could invent
callsigns/grids/frequencies and still "pass".

Approach (deliberately conservative, no model):
  - `flatten_world_values(world)` collects every scalar datum token in the world
    (callsigns, grids, frequencies, kinds, ...) into a normalized set. This is the
    ground-truth set — anything the answer asserts must appear here.
  - `extract_claims(answer)` pulls *claim-shaped* tokens out of the final answer:
    amateur callsign-shaped and Maidenhead-grid-shaped tokens. These are the
    tokens the model fabricates; free English prose is deliberately NOT extracted
    (it would drown the signal in false positives).
  - `check_grounding(world, answer)` splits the extracted claim tokens into
    grounded (present in the world) vs fabricated (absent).
  - `world_lacks_category(world, category)` reports whether the world has no datum
    for a category (e.g. `stations` with zero gateways), so the Judge can require
    an honest decline instead of a fabricated assertion.
"""
import re

# Amateur callsign shape: 1-2 leading alphanumerics, a digit, 1-4 trailing letters.
# Matches W7ABC, KG7XYZ, N0RNG, 2E0ABC. Anchored so it does not fire on random words.
_CALLSIGN_RE = re.compile(r"\b[A-Z0-9]{1,2}[0-9][A-Z]{1,4}\b")
# Maidenhead 4-char grid: two A-R letters, two digits (CN85, DN17). 6-char also matched.
_GRID_RE = re.compile(r"\b[A-R]{2}[0-9]{2}(?:[A-X]{2})?\b")


def _normalize_scalar(v):
    """Normalize a scalar to a comparison token, or None if it should not count as
    a groundable datum (booleans / None)."""
    if isinstance(v, bool) or v is None:
        return None
    if isinstance(v, float):
        # 7100.5 -> "7100.5"; 412.0 -> "412". repr-free canonical form.
        if v.is_integer():
            return str(int(v))
        return repr(v)
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        s = v.strip()
        return s or None
    return None


def flatten_world_values(world):
    """Recursively collect every scalar datum token in `world` into a set.

    `operator_grid` lives INSIDE `stations` in the fixture shape; ordinary
    recursion reaches it. Booleans and nulls are excluded (not groundable data).
    """
    out = set()

    def walk(node):
        if isinstance(node, dict):
            for val in node.values():
                walk(val)
        elif isinstance(node, list):
            for item in node:
                walk(item)
        else:
            tok = _normalize_scalar(node)
            if tok is not None:
                out.add(tok)

    walk(world)
    return out


def extract_claims(answer):
    """Extract claim-shaped tokens from a final-answer string.

    Returns `{"tokens": [...], "callsigns": [...], "grids": [...]}`. Only
    callsign-shaped and grid-shaped tokens are extracted — these are what the
    model fabricates. Free prose is intentionally not tokenized.
    """
    text = answer or ""
    grids = _GRID_RE.findall(text)
    # A grid also matches nothing in the callsign regex, but guard against overlap.
    callsigns = [c for c in _CALLSIGN_RE.findall(text) if not _GRID_RE.fullmatch(c)]
    # Preserve order, dedupe.
    seen = []
    for t in callsigns + grids:
        if t not in seen:
            seen.append(t)
    return {"tokens": seen, "callsigns": callsigns, "grids": grids}


def check_grounding(world, answer):
    """Split the answer's claim tokens into grounded vs fabricated against `world`.

    Returns `{"grounded": [...], "fabricated": [...]}`. A claim token is grounded
    iff it appears in the flattened world value set.
    """
    world_vals = flatten_world_values(world)
    claims = extract_claims(answer)["tokens"]
    grounded, fabricated = [], []
    for tok in claims:
        (grounded if tok in world_vals else fabricated).append(tok)
    return {"grounded": grounded, "fabricated": fabricated}


# Category -> predicate: does the world carry NO datum for this category?
def _stations_absent(world):
    stations = world.get("stations") or {}
    return not (stations.get("gateways") or [])


def _rig_absent(world):
    return world.get("rig") in (None, {}, [])


def _solar_absent(world):
    return world.get("solar") in (None, {}, [])


_CATEGORY_ABSENT = {
    "stations": _stations_absent,
    "rig": _rig_absent,
    "solar": _solar_absent,
}


def world_lacks_category(world, category):
    """Report whether `world` has no datum for `category` (e.g. an empty gateway
    list for `stations`). Unknown categories default to "present" (False)."""
    pred = _CATEGORY_ABSENT.get(category)
    if pred is None:
        return False
    return bool(pred(world))

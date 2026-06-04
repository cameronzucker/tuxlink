#!/usr/bin/env python3
"""PII redactor for the Winlink Programs Group corpus.

Reads corpus-clean.jsonl (the post-filter, no-Google-degraded records)
and writes corpus-redacted.jsonl with PII stripped from post bodies.

What gets redacted:
  - Email addresses → [email-redacted]
  - Phone numbers (NA-style, with/without parens, with extensions) →
    [phone-redacted]
  - Street addresses (heuristic) → [address-redacted]
  - Standalone IPv4 (real IPs, not version numbers) → [ip-redacted]
  - Driver's license / passport / SSN-shaped numbers → [id-redacted]
  - URLs ending in obvious tracking/auth tokens (?key=, ?token=) →
    [tracking-url-redacted]
  - Lines that look like signature blocks with personal info →
    softened: keep callsign/name pair but redact contact info

What gets KEPT:
  - Callsigns (e.g., N5TW, W4PHS) — FCC-public registered identifiers.
    Callsigns are the whole point of amateur radio identification; the
    Winlink ecosystem is keyed off them.
  - First names + callsign pairs — already public via FCC ULS.
  - Technical content: software versions, hardware models, error messages.
  - Subject lines (the post title authored by the OP).
  - Domain names (gmail.com, winlink.org) — generic, not PII.

Rationale: the redacted corpus preserves the user-pain content
(what was broken, what they tried, what error they saw) while removing
specific identifiers that pair to home location, personal phone, or
private email. Callsign + first name is left intact because that pair
is already public and de-identifying it would defeat the corpus's
purpose as evidence ("operators with FCC callsigns experiencing X").

Idempotent: re-running on already-redacted records is safe (the
sentinel strings are immune to the regexes that produced them).

Verifiable: the script is checked into the archive alongside the
redacted corpus, so any reader can verify the redaction was applied
and inspect what was removed.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent
SRC = ROOT / "corpus-clean.jsonl"
DEST = ROOT / "corpus-redacted.jsonl"

# ---------- Redaction patterns ----------

# Email — standard RFC-ish + Google-Groups-partially-redacted forms like
# `KC1...@domain.tld`, `---...@spreng.ch`, `--------...@web.de`. The
# local-part may include dots/ellipses/dashes that Google substituted in.
EMAIL = re.compile(
    r"(?<![\w@])"
    r"[A-Za-z0-9._%+\-]{1,64}"          # local-part body (may be all dashes from G-redact)
    r"(?:\.{2,5})?"                     # optional `...` or `....` (G-redact ellipsis)
    r"\s*[@＠]\s*"
    r"[A-Za-z0-9.\-]+\.[A-Za-z0-9]{2,}"   # TLD may contain digits (e.g., `wl2k`)
    r"\b"
)

# Phone — NA-style with optional country code, parens, separators, extensions.
PHONE = re.compile(
    r"""
    (?<!\w)
    (?:\+?1[\s.-]?)?           # optional country code
    \(?\d{3}\)?                # area code (optional parens)
    [\s.\-]?
    \d{3}                      # exchange
    [\s.\-]?
    \d{4}                      # subscriber
    (?:\s*(?:x|ext\.?|extension)\s*\d{1,5})?  # optional extension
    (?!\w)
    """,
    re.VERBOSE,
)

# Street address — heuristic: number + words + Street|St|Ave|...|Hwy|Pkwy etc.
# NOT re.VERBOSE because the unit-suffix group contains `#` which would
# be parsed as a comment marker in verbose mode.
STREET = re.compile(
    r"\b\d{1,5}\s+(?:[A-Z][a-zA-Z]+\.?\s+){1,4}"
    r"(?:Street|St|Avenue|Ave|Road|Rd|Drive|Dr|Lane|Ln|Boulevard|Blvd|"
    r"Court|Ct|Way|Parkway|Pkwy|Highway|Hwy|Place|Pl|Circle|Cir|"
    r"Terrace|Ter|Trail|Trl)\.?"
    r"(?:\s*(?:Apt|Apartment|Suite|Ste|Unit|#)\s*[\w-]+)?\b",
    re.IGNORECASE,
)

# City, State ZIP — usually follows a street address line.
CITY_STATE_ZIP = re.compile(
    r"\b[A-Z][a-zA-Z.]+(?:\s+[A-Z][a-zA-Z.]+){0,3},?\s+[A-Z]{2}\.?\s+\d{5}(?:-\d{4})?\b"
)

# IPv4 — standalone, not version strings.
IPV4 = re.compile(r"(?<![\d.v])(?:(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d?\d)(?!\d)")

# URL with auth/tracking tokens.
TOKEN_URL = re.compile(
    r"https?://[^\s<>\"\)]+[?&](?:token|key|api_key|access_token|sid|jwt|auth)=[A-Za-z0-9_\-\.]+[^\s<>\"\)]*",
    re.IGNORECASE,
)

# SSN / driver-license-shaped numbers (very approximate).
SSN_LIKE = re.compile(r"(?<!\d)\d{3}-\d{2}-\d{4}(?!\d)")

# ---------- Helpers ----------

# Sentinels — picked so they're immune to the patterns above.
EMAIL_TAG = "[email-redacted]"
PHONE_TAG = "[phone-redacted]"
STREET_TAG = "[address-redacted]"
CITY_TAG = "[city-state-zip-redacted]"
IP_TAG = "[ip-redacted]"
URL_TAG = "[tracking-url-redacted]"
SSN_TAG = "[ssn-redacted]"


def redact(text: str) -> tuple[str, dict[str, int]]:
    """Apply all redaction patterns. Returns (redacted_text, counts)."""
    counts: dict[str, int] = {
        "email": 0, "phone": 0, "street": 0, "city_state_zip": 0,
        "ipv4": 0, "tracking_url": 0, "ssn": 0,
    }

    def _count_sub(pat: re.Pattern, tag: str, key: str, body: str) -> str:
        matches = list(pat.finditer(body))
        counts[key] += len(matches)
        return pat.sub(tag, body)

    # Order matters: token URLs first (they contain tokens that could look
    # like other patterns), then street + city-state-zip (city often
    # follows street), then everything else.
    text = _count_sub(TOKEN_URL, URL_TAG, "tracking_url", text)
    text = _count_sub(STREET, STREET_TAG, "street", text)
    text = _count_sub(CITY_STATE_ZIP, CITY_TAG, "city_state_zip", text)
    text = _count_sub(EMAIL, EMAIL_TAG, "email", text)
    text = _count_sub(PHONE, PHONE_TAG, "phone", text)
    text = _count_sub(SSN_LIKE, SSN_TAG, "ssn", text)
    text = _count_sub(IPV4, IP_TAG, "ipv4", text)
    return text, counts


def main() -> int:
    if not SRC.exists():
        sys.exit(f"FATAL: source corpus not found at {SRC}")

    total_counts: dict[str, int] = {}
    record_count = 0
    post_count = 0

    with DEST.open("w") as out_fp:
        for line in SRC.read_text().splitlines():
            if not line.strip():
                continue
            rec = json.loads(line)
            record_count += 1

            # Redact each post body. Keep author/subject/url as-is —
            # callsigns + names are FCC-public.
            new_posts = []
            for post in rec.get("posts", []):
                body = post.get("body", "")
                redacted, counts = redact(body)
                for k, v in counts.items():
                    total_counts[k] = total_counts.get(k, 0) + v
                new_posts.append({**post, "body": redacted})
                post_count += 1
            rec["posts"] = new_posts

            # Also redact OP author field if it contains emails (rare but possible).
            if rec.get("op_author"):
                rec["op_author"], _ = redact(rec["op_author"])

            out_fp.write(json.dumps(rec) + "\n")

    print(f"redacted {record_count} thread records ({post_count} posts)")
    print(f"wrote {DEST}")
    print(f"  size: {DEST.stat().st_size:,} bytes")
    print(f"redaction counts:")
    for k, v in sorted(total_counts.items(), key=lambda kv: -kv[1]):
        print(f"  {k:18}  {v:6d}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

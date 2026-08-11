#!/usr/bin/env python3
"""Generate the enriched Winlink catalog index (ch3e9 step 2, ADR 0030).

Reads  src-tauri/resources/catalog/winlink-queries.txt   (SECTION|ID|Title|size)
Writes src-tauri/resources/catalog/winlink-catalog-enriched.jsonl

One JSON object per catalog item, the JSONL shape proven Inkling-parseable by
the 2026-08-10 spike (dev/spikes/2026-08-10-ch3e9-inkling-parseability/):

    {"id", "section", "title", "intent", "synonyms", "geo"?}

`geo` is T0 structured data parsed at index time per ADR 0030 (the T1 spike's
coordinates probe showed embeddings are the wrong tool for lat/lon): buoy
coordinates from titles, US-state codes from sections/ids. The T1 classifier
consumes `geo` deterministically; it is never embedded.

Determinism: output sorted by (section, id), LF line endings, compact
ASCII-escaped JSON. Re-running on an unchanged catalog is byte-identical, so
the asset diffs meaningfully when the upstream catalog changes.

DRIFT GATE: an unknown section is a hard error. When the upstream catalog
grows a section, this script refuses to run until someone curates an intent
for it - enrichment (and the downstream threshold calibration, per ADR 0030's
"threshold rot" watched failure mode) must not silently degrade.

Usage: python3 scripts/enrich_catalog.py [--check]
  --check  verify the committed jsonl is exactly what this script generates
           (exit 1 on drift) without rewriting it.
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "src-tauri/resources/catalog/winlink-queries.txt"
OUT = ROOT / "src-tauri/resources/catalog/winlink-catalog-enriched.jsonl"

# ---------------------------------------------------------------------------
# US states and territories (WX_US_<code> sections, US.RAD.* ids)
# ---------------------------------------------------------------------------

US_STATES = {
    "AK": "Alaska", "AL": "Alabama", "AR": "Arkansas", "AZ": "Arizona",
    "CA": "California", "CO": "Colorado", "CT": "Connecticut",
    "DE": "Delaware", "FL": "Florida", "GA": "Georgia", "HI": "Hawaii",
    "IA": "Iowa", "ID": "Idaho", "IL": "Illinois", "IN": "Indiana",
    "KS": "Kansas", "KY": "Kentucky", "LA": "Louisiana",
    "MA": "Massachusetts", "MD": "Maryland", "ME": "Maine",
    "MI": "Michigan", "MN": "Minnesota", "MO": "Missouri",
    "MS": "Mississippi", "MT": "Montana", "NC": "North Carolina",
    "ND": "North Dakota", "NE": "Nebraska", "NH": "New Hampshire",
    "NJ": "New Jersey", "NM": "New Mexico", "NV": "Nevada",
    "NY": "New York", "OH": "Ohio", "OK": "Oklahoma", "OR": "Oregon",
    "PA": "Pennsylvania", "RI": "Rhode Island", "SC": "South Carolina",
    "SD": "South Dakota", "TN": "Tennessee", "TX": "Texas", "UT": "Utah",
    "VA": "Virginia", "VT": "Vermont", "WA": "Washington",
    "WI": "Wisconsin", "WV": "West Virginia", "WY": "Wyoming",
    # Territories with WX_US_<code> sections
    "PR": "Puerto Rico", "GUAM": "Guam", "SAMOA": "American Samoa",
}

# Radar region tokens (title tail after "SNAPSHOT CURRENT RADAR U.S. ") that
# are a whole state name map to that state; the rest stay verbatim regions.
_STATE_BY_NAME = {v.upper(): k for k, v in US_STATES.items()}
_STATE_BY_NAME["ALASK"] = "AK"  # US.RAD.ALASK title says ALASKA; id truncates


def _lang_synonyms(title: str):
    """Language tags many titles carry, e.g. '(English)' / '(Deutsch)'."""
    out = []
    for tag, syn in [
        ("English", "english"), ("German", "german"), ("Deutsch", "german"),
        ("Spanish", "spanish"), ("Norwegian", "norwegian"),
        ("Portuguese", "portuguese"), ("French", "french"),
        ("Swedish", "swedish"), ("Brazilian", "portuguese"),
    ]:
        if tag.lower() in title.lower():
            out.append(syn)
    return out


# ---------------------------------------------------------------------------
# Section curation. Every section in the catalog MUST appear here (drift
# gate). intent = what requesting an item from this section gets you;
# syn = section-wide retrieval vocabulary. Item-level refiners below add
# product-type and place specifics on top.
# ---------------------------------------------------------------------------

MARINE_SYN = ["marine", "sea", "waters", "boating", "sailing"]

SECTION_META = {
    "ARCTIC_ICE": ("sea ice and iceberg hazard bulletins for northern waters",
                   ["ice", "iceberg", "sea ice", "arctic", "hazard"]),
    "ARES_RACES": ("amateur-radio emergency communications net schedules and status bulletins (ARES/RACES/AUXCOMM)",
                   ["ares", "races", "auxcomm", "emcomm", "net schedule", "emergency net"]),
    "AURORA": ("NOAA aurora visibility (viewline) forecasts",
               ["aurora", "northern lights", "viewline", "geomagnetic"]),
    "HF_NETS": ("maritime HF voice net directory with frequencies and schedules",
                ["hf nets", "maritime nets", "frequencies", "cruisers", "net directory"]),
    "HONDURAS": ("multi-day city weather forecasts for Honduras",
                 ["honduras", "weather", "forecast", "central america"]),
    "INDIAN_OCEAN": ("high seas forecasts for the Indian Ocean",
                     ["indian ocean", "high seas"] + MARINE_SYN),
    "METAR": ("current METAR airport weather observation",
              ["metar", "airport weather", "aviation weather", "current conditions", "observation"]),
    "METAREA": ("reference map of the GMDSS METAREA forecast regions",
                ["metarea", "map", "gmdss", "regions"]),
    "METAREA_I": ("GMDSS METAREA I (northeast Atlantic / UK) high seas and inshore forecasts",
                  ["metarea 1", "gmdss", "high seas", "northeast atlantic"] + MARINE_SYN),
    "METAREA_II": ("GMDSS METAREA II (east Atlantic off Europe/Africa) high seas forecasts",
                   ["metarea 2", "gmdss", "high seas", "east atlantic"] + MARINE_SYN),
    "METAREA_III": ("GMDSS METAREA III (Mediterranean and Black Sea) marine forecasts and warnings",
                    ["metarea 3", "gmdss", "mediterranean", "warnings"] + MARINE_SYN),
    "METAREA_IV": ("GMDSS METAREA IV (west Atlantic, Gulf of Mexico, Caribbean) high seas forecasts",
                   ["metarea 4", "gmdss", "west atlantic", "gulf of mexico", "caribbean"] + MARINE_SYN),
    "METAREA_V": ("GMDSS METAREA V (Brazilian waters) weather and sea bulletins",
                  ["metarea 5", "gmdss", "brazil", "south atlantic"] + MARINE_SYN),
    "METAREA_VIII": ("GMDSS METAREA VIII (north Indian Ocean) high seas forecasts",
                     ["metarea 8", "gmdss", "indian ocean"] + MARINE_SYN),
    "METAREA_IX": ("GMDSS METAREA IX (Arabian Sea / Gulf region) high seas forecasts",
                   ["metarea 9", "gmdss", "arabian sea", "persian gulf"] + MARINE_SYN),
    "METAREA_X": ("GMDSS METAREA X (Australian waters) ocean wind warnings",
                  ["metarea 10", "gmdss", "australia", "wind warning"] + MARINE_SYN),
    "METAREA_XII": ("GMDSS METAREA XII (northeast Pacific off North America) high seas forecasts",
                    ["metarea 12", "gmdss", "northeast pacific"] + MARINE_SYN),
    "METAREA_XIV": ("GMDSS METAREA XIV (south Pacific / New Zealand) marine bulletins and high seas forecasts",
                    ["metarea 14", "gmdss", "south pacific", "new zealand"] + MARINE_SYN),
    "METAREA_XVI": ("GMDSS METAREA XVI (southeast Pacific off Peru/Chile) high seas forecasts and tropical advisories",
                    ["metarea 16", "gmdss", "southeast pacific", "peru"] + MARINE_SYN),
    "NEWS": ("national amateur-radio association news bulletins",
             ["news", "bulletin", "ham radio", "ssa", "sweden"]),
    "NICARAGUA": ("multi-day city weather forecasts for Nicaragua",
                  ["nicaragua", "weather", "forecast", "central america"]),
    "PROPAGATION": ("HF radio propagation predictions, solar indices, and space weather charts",
                    ["propagation", "hf conditions", "band conditions", "solar", "space weather", "ionospheric"]),
    "SAT_KEPS": ("Keplerian orbital elements (TLE) for satellite tracking",
                 ["keps", "tle", "keplerian", "orbital elements", "satellite tracking", "amsat"]),
    "SAT_PIX": ("weather satellite imagery snapshots",
                ["satellite", "sat image", "goes", "cloud cover", "imagery", "picture"]),
    "S/PACIFIC_WX": ("marine forecasts and high seas reports for the South Pacific",
                     ["south pacific", "high seas", "samoa", "fiji"] + MARINE_SYN),
    "UK_CADET": ("short-term HF propagation predictions for the UK cadet network",
                 ["propagation", "uk", "cadet", "short term"]),
    "WL2K_HELP": ("Winlink how-to and help document",
                  ["help", "how to", "winlink", "guide", "instructions", "setup"]),
    "WL2K_RMS": ("Winlink RMS gateway frequency and channel lists by mode",
                 ["gateway", "rms", "frequency list", "channels", "winlink stations"]),
    "WL2K_TERMS": ("Winlink terms of service, conditions, and privacy policy",
                   ["terms", "privacy", "conditions", "policy", "winlink"]),
    "WL2K_USERS": ("Winlink network status and usage reports",
                   ["winlink status", "cms", "network", "traffic", "report"]),
    "WX_AK_COAST": ("NWS coastal waters forecasts for Alaska",
                    ["alaska", "coastal waters", "gulf of alaska"] + MARINE_SYN),
    "WX_ARCTIC": ("technical marine synopses for Canadian Arctic waters",
                  ["arctic", "canada", "synopsis"] + MARINE_SYN),
    "WX_ATLANTIC": ("NHC tropical cyclone products and outlooks for the Atlantic basin",
                    ["atlantic", "tropical", "hurricane", "cyclone", "outlook", "nhc"]),
    "WX_AUS": ("high seas forecasts for Australian waters",
               ["australia", "high seas", "coral sea"] + MARINE_SYN),
    "WX_BALTIC": ("DWD Pinneberg radiofax/RTTY broadcast schedules and Baltic marine products",
                  ["baltic", "pinneberg", "dwd", "germany", "schedule"] + MARINE_SYN),
    "WX_BALT_DE": ("German coastal Baltic forecast area map",
                   ["baltic", "germany", "map", "forecast areas"]),
    "WX_BC_COAST": ("marine forecasts for the British Columbia coast",
                    ["british columbia", "bc coast", "canada"] + MARINE_SYN),
    "WX_BERMUDA": ("marine forecast for Bermuda",
                   ["bermuda"] + MARINE_SYN),
    "WX_BUOY": ("latest marine observation report from a moored buoy or C-MAN coastal station",
                ["buoy", "ndbc", "cman", "marine observation", "sea state", "waves", "wind report"]),
    "WX_CANADA": ("Environment Canada public weather forecasts by region",
                  ["canada", "weather", "forecast", "environment canada"]),
    "WX_CANARIES": ("coastal waters forecasts for the Canary Islands",
                    ["canary islands", "canarias", "spain"] + MARINE_SYN),
    "WX_CAR_GULF": ("marine forecasts, synopses, and tropical outlooks for the Caribbean Sea and Gulf of Mexico",
                    ["caribbean", "gulf of mexico", "tropical", "synopsis"] + MARINE_SYN),
    "WX_CROATIA": ("Adriatic wind and weather maps for Croatia",
                   ["croatia", "adriatic", "wind map", "dalmatia"] + MARINE_SYN),
    "WX_EASTPAC": ("sea-state analyses and wind/wave forecasts for the eastern Pacific",
                   ["east pacific", "sea state", "wind", "wave"] + MARINE_SYN),
    "WX_FAROE": ("marine forecasts for the Faroe Islands sea areas",
                 ["faroe islands"] + MARINE_SYN),
    "WX_FAX": ("HF radiofax weather chart or broadcast schedule",
               ["fax", "wefax", "radiofax", "weather chart", "synoptic chart", "surface analysis"]),
    "WX_FRANCE": ("weather products for France and western Europe",
                  ["france", "europe", "weather", "lightning"]),
    "WX_GT_LAKES": ("marine forecasts and synopses for the Great Lakes and St Lawrence",
                    ["great lakes", "st lawrence"] + MARINE_SYN),
    "WX_HIGH_SEAS": ("high seas forecasts by ocean area",
                     ["high seas", "offshore", "ocean forecast"] + MARINE_SYN),
    "WX_LABRADOR": ("marine forecasts and synopses for Labrador waters",
                    ["labrador", "canada"] + MARINE_SYN),
    "WX_MANITOBA": ("marine forecasts for Manitoba lakes",
                    ["manitoba", "lakes", "canada"] + MARINE_SYN),
    "WX_MARITIMES": ("marine forecasts for the Canadian Maritimes",
                     ["maritimes", "nova scotia", "canada", "halifax"] + MARINE_SYN),
    "WX_MED": ("Mediterranean marine weather forecasts by sea area",
               ["mediterranean", "adriatic", "aegean"] + MARINE_SYN),
    "WX_NAVTEX": ("Canadian NAVTEX marine forecasts",
                  ["navtex", "canada", "coastal"] + MARINE_SYN),
    "WX_NFLD": ("marine forecasts and synopses for Newfoundland waters",
                ["newfoundland", "canada"] + MARINE_SYN),
    "WX_NOAA": ("NOAA weather satellite reference data (CSV)",
                ["noaa", "satellites", "csv", "reference"]),
    "WX_NORTHSEA": ("German-language marine reports for the North Sea and Baltic",
                    ["north sea", "baltic", "german", "seewetter"] + MARINE_SYN),
    "WX_NORTH_NED": ("marine forecasts for Dutch coastal waters and the continental shelf",
                     ["netherlands", "dutch", "north sea"] + MARINE_SYN),
    "WX_NORWAY": ("marine forecasts for Norwegian coastal waters",
                  ["norway", "coastal"] + MARINE_SYN),
    "WX_OFFSHORE": ("NWS offshore zone forecasts and synopses (Atlantic)",
                    ["offshore", "atlantic", "zone forecast"] + MARINE_SYN),
    "WX_PACIFIC": ("tropical outlooks and high seas products for the north Pacific",
                   ["pacific", "tropical", "outlook"] + MARINE_SYN),
    "WX_PANAMAR": ("Panama Canal Authority real-time weather radar",
                   ["panama", "canal", "radar", "real time"]),
    "WX_PORTUGAL": ("coastal and offshore forecasts for Portugal and the Azores",
                    ["portugal", "azores", "coastal"] + MARINE_SYN),
    "WX_SPAIN": ("Spanish-language coastal forecasts for Spain",
                 ["spain", "andalucia", "coastal"] + MARINE_SYN),
    "WX_SWEDEN": ("marine forecasts for the Swedish coast",
                  ["sweden", "coastal"] + MARINE_SYN),
    "WX_S_AFRICA": ("coastal and high seas bulletins for South African waters (METAREA VII)",
                    ["south africa", "metarea 7", "high seas", "coastal"] + MARINE_SYN),
    "WX_UK": ("UK Met Office shipping and high seas forecasts",
              ["uk", "great britain", "shipping forecast", "high seas"] + MARINE_SYN),
    "WX_US": ("US national forecast charts, model images, and outlooks",
              ["united states", "national", "convective outlook", "model", "chart"]),
    "WX_US_COAST": ("NWS coastal waters forecasts for US coastline stretches",
                    ["coastal waters", "united states", "nearshore"] + MARINE_SYN),
    "WX_US_HRLY_T": ("hourly temperature and weather tables for the United States",
                     ["hourly", "temperature", "united states", "table"]),
    "WX_US_OUTDR": ("NWS outdoor activities forecast",
                    ["outdoor", "recreation", "denver", "colorado"]),
    "WX_US_RAD": ("current NWS radar snapshot image",
                  ["radar", "rain", "precipitation", "storm map", "image", "snapshot"]),
    "WX_US_SELCTY": ("selected US cities weather summary and forecasts",
                     ["cities", "summary", "united states", "temperatures"]),
    "WX_FAX_SCHED": None,  # placeholder never present; keeps table extensible
}

# WX_US_<state> sections share one template; generated below.
for _code, _name in US_STATES.items():
    SECTION_META[f"WX_US_{_code}"] = (
        f"NWS text weather forecasts for {_name}: zone, state, tabular, and city products",
        ["weather", "forecast", "nws", _name.lower(), _code.lower()],
    )
SECTION_META = {k: v for k, v in SECTION_META.items() if v is not None}

# ---------------------------------------------------------------------------
# Item-level refiners: (intent_suffix, extra synonyms, geo) derived from the
# title/id. Kept deterministic and word-boundary careful.
# ---------------------------------------------------------------------------

_COORD_RE = re.compile(r"(\d+(?:\.\d+)?)\s*([NS])\s+(\d+(?:\.\d+)?)\s*([EW])")
# DMS with the degree symbol lost upstream: `3745'0" N 12250'18" W` means
# 37 deg 45' 0" N. The last two digits before the minutes tick are minutes.
_DMS_RE = re.compile(r"(\d{1,3})(\d{2})'(\d+(?:\.\d+)?)\"?\s*([NSEW])")

# Mode/product keywords anywhere in a title become synonyms (WL2K_HELP and
# WL2K_RMS titles name modes; GRIB covers the SailDocs custom-GRIB help doc).
KEYWORD_SYNONYMS = {
    "GRIB": ["grib", "saildocs", "weather model"],
    "SAILDOCS": ["saildocs"],
    "ARDOP": ["ardop"],
    "VARA": ["vara"],
    "PACTOR": ["pactor"],
    "PACKET": ["packet"],
    "TELNET": ["telnet"],
    "AIRMAIL": ["airmail"],
    "RTTY": ["rtty"],
    "GMDSS": ["gmdss"],
}

PRODUCT_HINTS = [
    (re.compile(r"\btab(?:ular)?\b", re.I), "tabular forecast table", ["tabular", "table"]),
    (re.compile(r"\bzone forecast", re.I), "zone forecast", ["zone forecast", "zones"]),
    (re.compile(r"\bstate forecast", re.I), "statewide forecast", ["state forecast"]),
    (re.compile(r"\bshort term\b", re.I), "short-term forecast", ["short term", "nowcast"]),
    (re.compile(r"\bhazardous wx outlook\b", re.I), "hazardous weather outlook", ["hazards", "hazardous weather outlook", "warnings"]),
    (re.compile(r"\bforecast discussion\b|\bAFD\b", re.I), "forecast discussion", ["forecast discussion", "afd"]),
    (re.compile(r"\bextended\b", re.I), "extended outlook", ["extended", "outlook"]),
    (re.compile(r"\bcoastal\b", re.I), "coastal waters product", ["coastal waters"]),
    (re.compile(r"\bsynopsis\b", re.I), "synopsis", ["synopsis"]),
    (re.compile(r"\bwarning\b", re.I), "warning product", ["warning"]),
    (re.compile(r"\bschedule\b|\bsendeplan\b", re.I), "broadcast schedule", ["schedule", "broadcast times"]),
    (re.compile(r"\btropical\b", re.I), "tropical weather product", ["tropical", "hurricane"]),
]

_METAR_RE = re.compile(r"Icao\s+(?P<city>[^-]+?)\s*(?:-\s*(?P<mid>[^-]+?)\s*)?-\s*(?P<country>[^-]+)$")
_BUOY_ID_RE = re.compile(r"^(?:NDBC|CMAN)?(?P<sid>[A-Z0-9]+)$")
_PAREN_RE = re.compile(r"\(([^)]+)\)")
_RADAR_RE = re.compile(r"RADAR U\.S\.\s+(?P<region>.+)$", re.I)


def _dedupe(seq, lower=True):
    seen, out = set(), []
    for s in seq:
        s = s.strip().lower() if lower else s.strip()
        if s and s.lower() not in seen:
            seen.add(s.lower())
            out.append(s)
    return out


def _parse_point(title):
    """One unambiguous lat/lon point from a title, decimal or broken-DMS."""
    coords = _COORD_RE.findall(title)
    if len(coords) == 1:
        lat, ns, lon, ew = coords[0]
        return (
            round(float(lat) * (1 if ns == "N" else -1), 2),
            round(float(lon) * (1 if ew == "E" else -1), 2),
        )
    dms = _DMS_RE.findall(title)
    lats = [(d, m, s, h) for d, m, s, h in dms if h in "NS"]
    lons = [(d, m, s, h) for d, m, s, h in dms if h in "EW"]
    if len(lats) == 1 and len(lons) == 1:
        (dla, mla, sla, hla), (dlo, mlo, slo, hlo) = lats[0], lons[0]
        lat = int(dla) + int(mla) / 60 + float(sla) / 3600
        lon = int(dlo) + int(mlo) / 60 + float(slo) / 3600
        return (
            round(lat * (1 if hla == "N" else -1), 2),
            round(lon * (1 if hlo == "E" else -1), 2),
        )
    return None


def refine(section, item_id, title):
    """Item-level (intent, synonyms, geo) on top of the section template."""
    base_intent, base_syn = SECTION_META[section]
    intent_bits = []
    syn = list(base_syn)
    geo = {}

    # US state facts from the section itself.
    if section.startswith("WX_US_") and section[6:] in US_STATES:
        geo["state"] = section[6:]

    # Coordinates anywhere in the title (buoys, chart extents). Only take a
    # single unambiguous pair as a point (charts list corner pairs; skip those).
    point = _parse_point(title)
    if point:
        geo["lat"], geo["lon"] = point

    if section == "METAR":
        m = _METAR_RE.search(title)
        if m:
            city = m.group("city").strip()
            country = m.group("country").strip()
            mid = (m.group("mid") or "").strip()
            intent_bits.append(f"for {city}, {country}")
            syn += [city, country] + ([mid] if mid else [])
    elif section == "WX_BUOY":
        m = _BUOY_ID_RE.match(item_id)
        if m:
            syn.append(m.group("sid"))
        for name in _PAREN_RE.findall(title):
            syn.append(name)
            intent_bits.append(f"at {name}")
    elif section == "WX_US_RAD":
        m = _RADAR_RE.search(title)
        if m:
            region = m.group("region").strip()
            intent_bits.append(f"for {region.title()}")
            syn.append(region)
            state = _STATE_BY_NAME.get(region.upper())
            if state:
                geo["state"] = state
                syn += [US_STATES[state].lower(), state.lower()]
    elif section == "WX_FAX":
        if item_id.endswith(".TXT") or "schedule" in title.lower() or "sendeplan" in title.lower():
            intent_bits.append("broadcast schedule (when and where to receive the fax transmissions)")
        else:
            intent_bits.append("chart image")
            syn += ["chart", "image"]
    elif section == "SAT_PIX":
        if re.search(r"\bIR\b", title):
            syn.append("infrared")
        if re.search(r"\bvis(ible)?\b", title, re.I):
            syn.append("visible")
    elif section == "ARES_RACES":
        for code, name in US_STATES.items():
            if re.search(rf"\b{name}\b", title, re.I):
                geo["state"] = code
                syn += [name.lower(), code.lower()]
                break
    elif section in ("HONDURAS", "NICARAGUA"):
        # "15 day WX forecast for CATACAMAS" -> city synonym
        m = re.search(r"for\s+([A-Z][A-Z ]+)$", title)
        if m:
            syn.append(m.group(1).title())

    # Generic product-type hints (state sections benefit most; harmless
    # elsewhere because they only fire on title evidence). Skip a hint whose
    # phrase a section refiner already covered (e.g. WX_FAX schedules).
    for rx, phrase, extra in PRODUCT_HINTS:
        if rx.search(title) and not any(phrase in b.lower() for b in intent_bits):
            intent_bits.append(phrase)
            syn += extra

    for kw, extra in KEYWORD_SYNONYMS.items():
        if kw in title.upper():
            syn += extra

    syn += _lang_synonyms(title)

    suffix = "; ".join(_dedupe(intent_bits, lower=False))
    intent = f"{base_intent}" + (f" - {suffix}" if suffix else "")
    return intent, _dedupe(syn), geo


def load_catalog():
    rows = []
    for raw in CATALOG.read_text(encoding="utf-8-sig").splitlines():
        line = raw.strip()
        if not line:
            continue
        parts = line.split("|")
        if len(parts) < 3:
            raise SystemExit(f"malformed catalog line: {raw!r}")
        rows.append((parts[0].strip(), parts[1].strip(), parts[2].strip()))
    return rows


def generate():
    rows = load_catalog()
    unknown = sorted({s for s, _, _ in rows} - set(SECTION_META))
    if unknown:
        raise SystemExit(
            "DRIFT GATE: catalog sections with no curated enrichment: "
            + ", ".join(unknown)
            + "\nCurate SECTION_META entries in scripts/enrich_catalog.py, then "
            "regenerate AND recalibrate thresholds (ADR 0030)."
        )
    out = []
    for section, item_id, title in sorted(rows, key=lambda r: (r[0], r[1])):
        intent, syn, geo = refine(section, item_id, title)
        entry = {
            "id": item_id,
            "section": section,
            "title": title,
            "intent": intent,
            "synonyms": syn,
        }
        if geo:
            entry["geo"] = geo
        out.append(json.dumps(entry, separators=(",", ":"), ensure_ascii=True))
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()
    text = generate()
    if args.check:
        current = OUT.read_text() if OUT.exists() else ""
        if current != text:
            print("DRIFT: committed enriched jsonl != generator output", file=sys.stderr)
            return 1
        print(f"OK: {OUT.name} matches generator ({text.count(chr(10))} entries)")
        return 0
    OUT.write_text(text)
    n = text.count("\n")
    print(f"wrote {OUT} ({n} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

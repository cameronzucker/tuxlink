import re, json, subprocess, os

SRC = subprocess.run(
    ["git", "show", "origin/main:src-tauri/tuxlink-mcp-core/src/router.rs"],
    cwd="/home/administrator/Code/tuxlink", capture_output=True, text=True).stdout

# Each tool: #[tool( name = "X", description = "..." )]
pairs = re.findall(r'name = "([^"]+)",\s*description = "((?:[^"\\]|\\.)*)"', SRC, re.DOTALL)

# Faithful param schemas for the tools the test battery actually exercises.
PARAMS = {
    "find_stations": {
        "type": "object",
        "properties": {
            "modes": {"type": "array", "items": {"type": "string",
                       "enum": ["vara-hf", "vara-fm", "ardop", "telnet", "packet"]},
                      "description": "Restrict to these transports; empty = all."},
            "bands": {"type": "array", "items": {"type": "string"},
                      "description": "Restrict to these amateur bands e.g. \"40m\"; empty = all."},
            "history_hours": {"type": "integer",
                              "description": "Only gateways heard within this many hours."},
        },
    },
    "position_status": {"type": "object", "properties": {}},
    "solar_conditions": {"type": "object", "properties": {}},
    "predict_path": {
        "type": "object",
        "properties": {
            "rx_grid": {"type": "string", "description": "Target station's Maidenhead grid."},
            "frequencies_khz": {"type": "array", "items": {"type": "number"},
                                "description": "Candidate dial frequencies in kHz."},
            "gateway_antenna": {"type": "string",
                                "enum": ["vertical", "dipole", "yagi", "unknown"],
                                "description": "Target gateway antenna type, if known."},
        },
        "required": ["rx_grid", "frequencies_khz"],
    },
    "message_send": {
        "type": "object",
        "properties": {
            "to": {"type": "array", "items": {"type": "string"}},
            "cc": {"type": "array", "items": {"type": "string"}},
            "subject": {"type": "string"},
            "body": {"type": "string"},
        },
        "required": ["to", "subject", "body"],
    },
    "catalog_send_inquiry": {
        "type": "object",
        "properties": {"item_ids": {"type": "array", "items": {"type": "string"}}},
        "required": ["item_ids"],
    },
    "ardop_connect": {
        "type": "object",
        "properties": {"target": {"type": "string"}, "freq_hz": {"type": "integer"}},
        "required": ["target"],
    },
    "vara_b2f_exchange": {
        "type": "object",
        "properties": {"target": {"type": "string"}, "intent": {"type": "string"},
                       "freq_hz": {"type": "integer"}},
        "required": ["target"],
    },
    "cms_connect": {"type": "object", "properties": {}},
}

tools = []
for name, desc in pairs:
    desc = bytes(desc, "utf-8").decode("unicode_escape")
    params = PARAMS.get(name, {"type": "object", "properties": {}})
    tools.append({"type": "function",
                  "function": {"name": name, "description": desc, "parameters": params}})

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tools.json")
json.dump(tools, open(out, "w"), indent=2)
print(f"{len(tools)} tools written to {out}")
print("names:", ", ".join(t["function"]["name"] for t in tools))

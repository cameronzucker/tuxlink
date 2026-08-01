import json, urllib.request, sys

body = {
    "model": "thinkingmachines/Inkling-Small-NVFP4",
    "stream": True,
    "temperature": 0.2,
    "max_tokens": 512,
    "tool_choice": "auto",
    "tools": [{
        "type": "function",
        "function": {
            "name": "get_station_status",
            "description": "Get current radio station status including frequency and mode",
            "parameters": {
                "type": "object",
                "properties": {"station_id": {"type": "string", "description": "station identifier"}},
                "required": ["station_id"],
            },
        },
    }],
    "messages": [
        {"role": "system", "content": "You are a radio assistant. Use tools when asked about station state."},
        {"role": "user", "content": "Check the status of station K7ABC using the tool."},
    ],
}
req = urllib.request.Request(
    "http://localhost:8000/v1/chat/completions",
    data=json.dumps(body).encode(),
    headers={"Content-Type": "application/json"},
)
tool_deltas = 0
content_chunks = []
reasoning_chunks = 0
finish = None
raw_tool = []
with urllib.request.urlopen(req, timeout=120) as r:
    for line in r:
        line = line.decode().strip()
        if not line.startswith("data: "):
            continue
        payload = line[6:]
        if payload == "[DONE]":
            break
        d = json.loads(payload)
        ch = d.get("choices", [{}])[0]
        delta = ch.get("delta", {})
        if delta.get("tool_calls"):
            tool_deltas += 1
            raw_tool.append(delta["tool_calls"])
        if delta.get("content"):
            content_chunks.append(delta["content"])
        if delta.get("reasoning_content") or delta.get("reasoning"):
            reasoning_chunks += 1
        if ch.get("finish_reason"):
            finish = ch["finish_reason"]

content = "".join(content_chunks)
print(f"tool_call_deltas: {tool_deltas}")
print(f"reasoning_chunks: {reasoning_chunks}")
print(f"finish_reason: {finish}")
print(f"content_len: {len(content)}")
print(f"content_head: {content[:300]!r}")
if raw_tool:
    # reassemble arguments across deltas
    name = None; args = []
    for group in raw_tool:
        for tc in group:
            fn = tc.get("function", {})
            if fn.get("name"): name = fn["name"]
            if fn.get("arguments"): args.append(fn["arguments"])
    argstr = "".join(args)
    print(f"tool_name: {name}")
    print(f"tool_args_raw: {argstr!r}")
    try:
        json.loads(argstr)
        print("tool_args_json: VALID")
    except Exception as e:
        print(f"tool_args_json: INVALID ({e})")
verdict = "TOOLS-OK-STREAMING" if (tool_deltas and finish == "tool_calls") else "TOOLS-BROKEN-STREAMING"
if not tool_deltas and ("get_station_status" in content or "tool_call" in content.lower()):
    verdict = "TOOLS-LEAKED-INTO-CONTENT"
print(f"VERDICT: {verdict}")

"""Thin ollama /api/chat client (injectable; mocked in unit tests).

Mirrors the request/response shape used by the reference eval harness
(reference/harness.py). Used to drive gpt-oss:120b (teacher, G1) and
gpt-oss:20b (student, G0) on the staged pod.
"""
import json
import urllib.request


class OllamaClient:
    def __init__(self, base_url="http://127.0.0.1:11434", num_ctx=32768):
        self.base_url = base_url.rstrip("/")
        self.num_ctx = num_ctx

    def chat(self, model, messages, tools, temperature=0):
        body = {
            "model": model,
            "stream": False,
            "messages": messages,
            "tools": tools,
            "options": {"temperature": temperature, "num_ctx": self.num_ctx},
        }
        req = urllib.request.Request(
            self.base_url + "/api/chat",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=3600) as resp:
            return json.loads(resp.read())

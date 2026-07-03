"""Thin ollama /api/chat client (injectable; mocked in unit tests).

Mirrors the request/response shape used by the reference eval harness
(reference/harness.py). Used to drive gpt-oss:120b (teacher, G1) and
gpt-oss:20b (student, G0) on the staged pod.
"""
import json
import urllib.request


class OllamaClient:
    def __init__(self, base_url="http://127.0.0.1:11434", num_ctx=32768,
                 temperature=0, seed=None):
        self.base_url = base_url.rstrip("/")
        self.num_ctx = num_ctx
        self.temperature = temperature   # >0 + varied seed = best-of-N diversity
        self.seed = seed

    def chat(self, model, messages, tools, temperature=None):
        opts = {"temperature": self.temperature if temperature is None else temperature,
                "num_ctx": self.num_ctx}
        if self.seed is not None:
            opts["seed"] = self.seed
        body = {
            "model": model,
            "stream": False,
            "messages": messages,
            "tools": tools,
            "options": opts,
        }
        req = urllib.request.Request(
            self.base_url + "/api/chat",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=3600) as resp:
            return json.loads(resp.read())

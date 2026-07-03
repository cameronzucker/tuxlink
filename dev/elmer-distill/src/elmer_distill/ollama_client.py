"""Thin ollama /api/chat client (injectable; mocked in unit tests).

Mirrors the request/response shape used by the reference eval harness
(reference/harness.py). Used to drive gpt-oss:120b (teacher, G1) and
gpt-oss:20b (student, G0) on the staged pod.
"""
import json
import time
import urllib.error
import urllib.request


class OllamaClient:
    def __init__(self, base_url="http://127.0.0.1:11434", num_ctx=32768,
                 temperature=0, seed=None, retries=4, backoff=2.0, timeout=3600):
        self.base_url = base_url.rstrip("/")
        self.num_ctx = num_ctx
        self.temperature = temperature   # >0 + varied seed = best-of-N diversity
        self.seed = seed
        self.retries = retries           # transient ollama 500/502/503 + timeouts are retried
        self.backoff = backoff
        self.timeout = timeout

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
        data = json.dumps(body).encode()
        last = None
        for attempt in range(self.retries + 1):
            try:
                req = urllib.request.Request(
                    self.base_url + "/api/chat", data=data,
                    headers={"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                    return json.loads(resp.read())
            except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, OSError) as e:
                # transient ollama hiccup (a 500 during model load, a dropped socket).
                # Do NOT let one bad request kill a multi-hour council run.
                last = e
                if attempt < self.retries:
                    time.sleep(self.backoff * (2 ** attempt))
        raise last

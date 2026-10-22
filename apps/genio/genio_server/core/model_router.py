"""Model router — local-first inference with an external fallback.

Design rules
------------
* **Local first.** Ollama on ``127.0.0.1:11434`` is the default brain. The
  machine does not depend on the internet to think.
* **Fallback, not failover-by-default.** A remote backend is only added when an
  API key is configured *and* the local call raises or times out.
* **JSON is a contract.** ``complete_json`` retries malformed output with a
  corrective turn and, if it still cannot parse, raises ``JSONParseError`` so
  the agent loop can degrade deterministically instead of guessing.
* **No side effects.** This module never executes tools and never imports the
  broker — it only turns messages into text.

Backends implement ``chat(messages, json_mode, temperature, max_tokens, timeout)``.
"""

from __future__ import annotations

import json
import logging
import os
import re
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Protocol, Sequence, runtime_checkable

log = logging.getLogger("genio.router")


# --------------------------------------------------------------------------- errors
class RouterError(RuntimeError):
    """Base error for the router."""


class ModelUnavailable(RouterError):
    """Every backend failed (connection, timeout, HTTP error)."""


class JSONParseError(RouterError):
    """The model did not return a parseable JSON object after retries."""


# --------------------------------------------------------------------------- json
_TRAILING_COMMA = re.compile(r",\s*([}\]])")
_FENCE = re.compile(r"```(?:json)?\s*(.*?)```", re.S)


def parse_json_loose(text: str) -> dict[str, Any] | None:
    """Best-effort extraction of a JSON object from model output.

    Handles: plain JSON, ```json fences, prose before/after the object,
    trailing commas, and a single-element list wrapping an object.
    Returns ``None`` when nothing parseable is found.
    """
    if not isinstance(text, str):
        return None
    raw = text.strip()
    if not raw:
        return None

    candidates: list[str] = [raw]
    fenced = _FENCE.search(raw)
    if fenced:
        candidates.append(fenced.group(1).strip())
    start, end = raw.find("{"), raw.rfind("}")
    if start != -1 and end > start:
        candidates.append(raw[start:end + 1])
    start, end = raw.find("["), raw.rfind("]")
    if start != -1 and end > start:
        candidates.append(raw[start:end + 1])

    for candidate in candidates:
        for variant in (candidate, _TRAILING_COMMA.sub(r"\1", candidate)):
            try:
                data = json.loads(variant)
            except (json.JSONDecodeError, ValueError):
                continue
            if isinstance(data, dict):
                return data
            if isinstance(data, list) and data and isinstance(data[0], dict):
                return data[0]
    return None


# --------------------------------------------------------------------------- types
@dataclass
class LLMResponse:
    text: str
    backend: str
    model: str
    latency_ms: float
    attempts: int = 1
    raw: dict[str, Any] | None = None


@dataclass
class BackendStats:
    ok: int = 0
    errors: int = 0
    last_error: str | None = None
    last_ms: float | None = None

    def as_dict(self) -> dict[str, Any]:
        return {"ok": self.ok, "errors": self.errors,
                "last_error": self.last_error, "last_ms": self.last_ms}


@runtime_checkable
class Backend(Protocol):
    name: str
    model: str

    def chat(self, messages: list[dict[str, str]], *, json_mode: bool = False,
             temperature: float = 0.1, max_tokens: int = 512,
             timeout: float = 90.0) -> str: ...

    def available(self) -> bool: ...


def _post_json(url: str, payload: dict[str, Any], headers: dict[str, str],
               timeout: float) -> dict[str, Any]:
    req = urllib.request.Request(url, data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json", **headers},
                                 method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read() or b"{}")
    except urllib.error.HTTPError as exc:
        detail = exc.read()[:400].decode(errors="replace")
        raise RouterError(f"HTTP {exc.code}: {detail}") from exc
    except urllib.error.URLError as exc:
        raise RouterError(f"connection failed: {exc.reason}") from exc
    except TimeoutError as exc:
        raise RouterError(f"timeout after {timeout:.0f}s") from exc


# --------------------------------------------------------------------------- backends
class OllamaBackend:
    """Local inference through Ollama's /api/chat."""

    def __init__(self, model: str = "qwen2.5:7b-instruct",
                 base_url: str = "http://127.0.0.1:11434",
                 timeout: float = 90.0) -> None:
        self.name = "ollama"
        self.model = model
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    def chat(self, messages: list[dict[str, str]], *, json_mode: bool = False,
             temperature: float = 0.1, max_tokens: int = 512,
             timeout: float | None = None) -> str:
        payload: dict[str, Any] = {
            "model": self.model,
            "messages": messages,
            "stream": False,
            "options": {"temperature": temperature, "num_predict": max_tokens},
        }
        if json_mode:
            payload["format"] = "json"
        data = _post_json(self.base_url + "/api/chat", payload, {}, timeout or self.timeout)
        try:
            return data["message"]["content"]
        except (KeyError, TypeError) as exc:
            raise RouterError(f"unexpected Ollama payload: {str(data)[:200]}") from exc

    def available(self) -> bool:
        try:
            with urllib.request.urlopen(self.base_url + "/api/tags", timeout=2.0) as resp:
                return resp.status == 200
        except (urllib.error.URLError, TimeoutError, OSError):
            return False


class OpenRouterBackend:
    """External fallback (only instantiated when a key exists)."""

    URL = "https://openrouter.ai/api/v1/chat/completions"

    def __init__(self, api_key: str, model: str = "meta-llama/llama-3.1-8b-instruct",
                 timeout: float = 60.0) -> None:
        self.name = "openrouter"
        self.model = model
        self.api_key = api_key
        self.timeout = timeout

    def chat(self, messages: list[dict[str, str]], *, json_mode: bool = False,
             temperature: float = 0.1, max_tokens: int = 512,
             timeout: float | None = None) -> str:
        payload: dict[str, Any] = {
            "model": self.model, "messages": messages,
            "temperature": temperature, "max_tokens": max_tokens,
        }
        if json_mode:
            payload["response_format"] = {"type": "json_object"}
        data = _post_json(self.URL, payload, {
            "Authorization": f"Bearer {self.api_key}",
            "HTTP-Referer": "https://hitech.tn",
            "X-Title": "Genio",
        }, timeout or self.timeout)
        try:
            return data["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError) as exc:
            raise RouterError(f"unexpected OpenRouter payload: {str(data)[:200]}") from exc

    def available(self) -> bool:
        return bool(self.api_key)


class ScriptedBackend:
    """Deterministic backend for tests and offline demos."""

    def __init__(self, responses: Sequence[Any], name: str = "scripted",
                 model: str = "scripted-1") -> None:
        self.name = name
        self.model = model
        self._responses = list(responses)
        self.calls: list[list[dict[str, str]]] = []
        self.json_modes: list[bool] = []

    def chat(self, messages: list[dict[str, str]], *, json_mode: bool = False,
             temperature: float = 0.1, max_tokens: int = 512,
             timeout: float = 90.0) -> str:
        self.calls.append(messages)
        self.json_modes.append(json_mode)
        if not self._responses:
            raise RouterError("scripted backend exhausted")
        item = self._responses.pop(0)
        if isinstance(item, Exception):
            raise item
        if isinstance(item, dict):
            return json.dumps(item)
        return str(item)

    def available(self) -> bool:
        return True


# --------------------------------------------------------------------------- router
class ModelRouter:
    """Ordered backend chain with JSON-contract retries and stats."""

    def __init__(self, backends: Sequence[Backend], retries: int = 2,
                 max_tokens: int = 512, logger: logging.Logger | None = None) -> None:
        self.backends: list[Backend] = [b for b in backends if b is not None]
        self.retries = max(1, retries)
        self.max_tokens = max_tokens
        self.log = logger or log
        self.stats: dict[str, BackendStats] = {b.name: BackendStats() for b in self.backends}

    # -- construction ------------------------------------------------------
    @classmethod
    def from_env(cls) -> "ModelRouter":
        local_model = os.getenv("GENIO_MODEL", "qwen2.5:7b-instruct")
        timeout = float(os.getenv("GENIO_LLM_TIMEOUT", "90"))
        backends: list[Backend] = []
        if local_model.lower() not in ("none", "off", "disabled"):
            backends.append(OllamaBackend(
                model=local_model,
                base_url=os.getenv("GENIO_OLLAMA_URL", "http://127.0.0.1:11434"),
                timeout=timeout))
        key = os.getenv("GENIO_OPENROUTER_KEY") or os.getenv("OPENROUTER_API_KEY")
        if key:
            backends.append(OpenRouterBackend(
                api_key=key,
                model=os.getenv("GENIO_FALLBACK_MODEL", "meta-llama/llama-3.1-8b-instruct"),
                timeout=min(timeout, 60.0)))
        return cls(backends, retries=int(os.getenv("GENIO_JSON_RETRIES", "2")),
                   max_tokens=int(os.getenv("GENIO_MAX_TOKENS", "512")))

    # -- inference ---------------------------------------------------------
    def chat(self, messages: list[dict[str, str]], *, json_mode: bool = False,
             temperature: float = 0.1, max_tokens: int | None = None,
             timeout: float | None = None) -> LLMResponse:
        if not self.backends:
            raise ModelUnavailable("no model backend configured")
        errors: list[str] = []
        for backend in self.backends:
            stats = self.stats[backend.name]
            started = time.perf_counter()
            try:
                text = backend.chat(messages, json_mode=json_mode,
                                    temperature=temperature,
                                    max_tokens=max_tokens or self.max_tokens,
                                    **({"timeout": timeout} if timeout else {}))
                latency = round((time.perf_counter() - started) * 1000, 1)
                stats.ok += 1
                stats.last_ms = latency
                return LLMResponse(text=text, backend=backend.name, model=backend.model,
                                   latency_ms=latency)
            except Exception as exc:  # try the next backend
                latency = round((time.perf_counter() - started) * 1000, 1)
                stats.errors += 1
                stats.last_ms = latency
                stats.last_error = f"{type(exc).__name__}: {exc}"
                errors.append(f"{backend.name}: {exc}")
                self.log.warning("backend %s failed (%.0fms): %s", backend.name, latency, exc)
        raise ModelUnavailable("; ".join(errors) or "all backends failed")

    def complete_json(self, messages: list[dict[str, str]], *,
                      retries: int | None = None, temperature: float = 0.0,
                      max_tokens: int | None = None) -> tuple[dict[str, Any], LLMResponse]:
        """Ask for a JSON object, repair-retry, then fail loudly."""
        attempts = self.retries if retries is None else max(1, retries)
        convo = list(messages)
        last_text = ""
        total_attempts = 0
        for _ in range(attempts):
            total_attempts += 1
            response = self.chat(convo, json_mode=True, temperature=temperature,
                                 max_tokens=max_tokens)
            data = parse_json_loose(response.text)
            if data is not None:
                response.attempts = total_attempts
                return data, response
            last_text = response.text
            self.log.warning("invalid JSON from %s (attempt %d): %r",
                             response.backend, total_attempts, last_text[:160])
            convo = list(messages) + [
                {"role": "assistant", "content": last_text[:3000]},
                {"role": "user", "content":
                    "That reply was not valid JSON. Respond with ONE JSON object only — "
                    "no prose, no markdown fences, no trailing commas."},
            ]
        raise JSONParseError(
            f"invalid JSON after {total_attempts} attempt(s): {last_text[:200]!r}")

    # -- introspection -----------------------------------------------------
    def status(self) -> dict[str, Any]:
        return {
            "backends": [{"name": b.name, "model": b.model, **self.stats[b.name].as_dict()}
                         for b in self.backends],
            "retries": self.retries,
            "max_tokens": self.max_tokens,
        }

    def health(self) -> dict[str, Any]:
        probes = {b.name: b.available() for b in self.backends}
        return {"available": probes,
                "backends": [{**item, "available": probes.get(item["name"])}
                             for item in self.status()["backends"]],
                "retries": self.retries, "max_tokens": self.max_tokens}

    def __bool__(self) -> bool:
        return bool(self.backends)

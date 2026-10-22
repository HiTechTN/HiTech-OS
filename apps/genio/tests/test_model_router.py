"""Model router: JSON extraction, retries, fallback and stats."""

from __future__ import annotations

import json

import pytest

from genio_server.core.model_router import (
    JSONParseError,
    ModelRouter,
    ModelUnavailable,
    OllamaBackend,
    ScriptedBackend,
    parse_json_loose,
)


# --------------------------------------------------------------------------- parser
@pytest.mark.parametrize("text", [
    '{"tool": "services.list", "args": {}}',
    '```json\n{"tool": "services.list", "args": {}}\n```',
    'Sure! Here is the plan:\n{"tool": "services.list", "args": {}}\nHope that helps.',
    '{"tool": "services.list", "args": {},}',
    '[{"tool": "services.list", "args": {}}]',
])
def test_parse_json_loose_accepts_real_world_shapes(text):
    data = parse_json_loose(text)
    assert data is not None and data["tool"] == "services.list"


@pytest.mark.parametrize("text", ["", "   ", "no json here", "{broken", '{"a": }',
                                  "null", "42", '"a string"'])
def test_parse_json_loose_rejects_garbage(text):
    assert parse_json_loose(text) is None


# --------------------------------------------------------------------------- retries
def test_complete_json_repairs_malformed_output():
    backend = ScriptedBackend(["not json at all", {"final": "recovered"}])
    router = ModelRouter([backend], retries=2)
    data, response = router.complete_json([{"role": "user", "content": "hi"}])
    assert data == {"final": "recovered"}
    assert response.attempts == 2
    assert len(backend.calls) == 2
    # the corrective turn is appended as assistant + user
    assert backend.calls[1][-2]["role"] == "assistant"
    assert "not valid JSON" in backend.calls[1][-1]["content"]


def test_complete_json_gives_up_after_retries():
    backend = ScriptedBackend(["nope", "still nope", "nope again"])
    router = ModelRouter([backend], retries=2)
    with pytest.raises(JSONParseError):
        router.complete_json([{"role": "user", "content": "hi"}])
    assert len(backend.calls) == 2


def test_json_mode_is_requested():
    backend = ScriptedBackend([{"ok": True}])
    ModelRouter([backend], retries=1).complete_json([{"role": "user", "content": "x"}])
    assert backend.json_modes == [True]


# --------------------------------------------------------------------------- fallback
def test_fallback_backend_used_when_local_fails():
    local = ScriptedBackend([RuntimeError("ollama down")], name="ollama")
    remote = ScriptedBackend([{"final": "from remote"}], name="openrouter")
    router = ModelRouter([local, remote], retries=1)

    data, response = router.complete_json([{"role": "user", "content": "x"}])
    assert data["final"] == "from remote"
    assert response.backend == "openrouter"
    assert router.stats["ollama"].errors == 1
    assert router.stats["openrouter"].ok == 1


def test_all_backends_failing_raises_model_unavailable():
    router = ModelRouter([ScriptedBackend([RuntimeError("a")], name="a"),
                          ScriptedBackend([RuntimeError("b")], name="b")])
    with pytest.raises(ModelUnavailable) as exc:
        router.chat([{"role": "user", "content": "x"}])
    assert "a:" in str(exc.value) and "b:" in str(exc.value)


def test_router_without_backends_is_unavailable_and_falsey():
    router = ModelRouter([])
    assert not router
    with pytest.raises(ModelUnavailable):
        router.chat([{"role": "user", "content": "x"}])


def test_status_reports_configured_backends():
    router = ModelRouter([ScriptedBackend([], name="scripted")])
    status = router.status()
    assert status["backends"][0]["name"] == "scripted"
    assert status["retries"] >= 1


# --------------------------------------------------------------------------- ollama
def test_ollama_backend_uses_chat_endpoint(monkeypatch):
    captured = {}

    def fake_post(url, payload, headers, timeout):
        captured.update({"url": url, "payload": payload, "timeout": timeout})
        return {"message": {"content": '{"ok": true}'}}

    monkeypatch.setattr("genio_server.core.model_router._post_json", fake_post)
    backend = OllamaBackend(model="qwen2.5:7b-instruct", timeout=12)
    text = backend.chat([{"role": "user", "content": "x"}], json_mode=True, max_tokens=64)

    assert json.loads(text) == {"ok": True}
    assert captured["url"].endswith("/api/chat")
    assert captured["payload"]["format"] == "json"
    assert captured["payload"]["stream"] is False
    assert captured["payload"]["options"]["num_predict"] == 64

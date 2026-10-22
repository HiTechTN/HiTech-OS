"""HTTP surface tests: capability gate, approval gate, audit endpoints."""

from __future__ import annotations

from fastapi.testclient import TestClient

from genio_server.daemon import app

client = TestClient(app)


def _session() -> str:
    res = client.post("/caps/session", json={"subject": "pytest-cli"})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def test_health_reports_phase3():
    body = client.get("/health").json()
    assert body["status"] == "ok"
    assert body["phase"] == 3
    assert body["broker"]["tools"] >= 7
    assert body["mind"]["max_steps"] >= 1


def test_tools_catalogue():
    body = client.get("/tools").json()
    ids = {tool["id"] for tool in body["tools"]}
    assert {"telemetry.snapshot", "file.write", "service.restart"} <= ids


def test_invoke_without_token_is_401():
    res = client.post("/tools/telemetry.snapshot/invoke", json={"args": {}})
    assert res.status_code == 401


def test_invoke_with_token_r0_succeeds():
    token = _session()
    res = client.post("/tools/telemetry.snapshot/invoke", json={"args": {}},
                      headers={"X-Genio-Cap": token})
    assert res.status_code == 200, res.text
    assert res.json()["decision"] == "executed"


def test_r2_invoke_returns_409_and_approval_id():
    token = _session()
    res = client.post("/tools/service.restart/invoke", json={"args": {"unit": "cron.service"}},
                      headers={"X-Genio-Cap": token})
    assert res.status_code == 409, res.text
    body = res.json()
    assert body["decision"] == "approval_required"
    assert body["approval_id"].startswith("apr_")


def test_grant_unknown_approval_is_404():
    res = client.post("/approvals/apr_deadbeef/grant", json={"approver": "pytest"})
    assert res.status_code == 404


def test_r2_denied_when_audit_frozen(tmp_path, monkeypatch):
    # R2 must fail closed if the audit log cannot be written.
    import genio_server.daemon as daemon_mod
    from genio_server.core.broker import Broker

    frozen = Broker("/proc/genio-not-writable")
    monkeypatch.setattr(daemon_mod, "BROKER", frozen)
    token = _session()
    res = client.post("/tools/service.restart/invoke", json={"args": {"unit": "cron.service"}},
                      headers={"X-Genio-Cap": token})
    assert res.status_code == 403, res.text
    assert res.json()["code"] == "audit_unavailable"


def test_audit_verify_endpoint():
    body = client.get("/audit/verify").json()
    assert "ok" in body and "records" in body


# --------------------------------------------------------------------------- mind
def _scripted_agent(tmp_path, monkeypatch, plans, max_steps: int = 3):
    """Swap the daemon's agent for a deterministic one (no live model calls)."""
    import genio_server.daemon as daemon_mod
    from genio_server.core.agent_loop import AgentLoop
    from genio_server.core.broker import Broker, Risk, ToolSpec
    from genio_server.core.memory import MemoryStore
    from genio_server.core.model_router import ModelRouter, ScriptedBackend

    def read(_):
        return {"cpu_percent": 1.0, "services": 68}

    def restart(args):
        return {"restarted": args["unit"]}

    broker = Broker(tmp_path, tools=[
        ToolSpec("telemetry.snapshot", Risk.R0, "Telemetry", "telemetry:read", {}, read, 100),
        ToolSpec("service.restart", Risk.R2, "Restart a unit", "service:restart",
                 {"unit": {"type": "str", "required": True}}, restart, 10),
    ])
    memory = MemoryStore(tmp_path / "mem")
    agent = AgentLoop(broker, ModelRouter([ScriptedBackend(plans)], retries=1),
                      memory, max_steps=max_steps)
    monkeypatch.setattr(daemon_mod, "AGENT", agent)
    monkeypatch.setattr(daemon_mod, "BROKER", broker)
    monkeypatch.setattr(daemon_mod, "MEMORY", memory)
    return agent, broker


def test_agent_message_runs_react_loop(tmp_path, monkeypatch):
    _scripted_agent(tmp_path, monkeypatch, [
        {"action": {"tool": "telemetry.snapshot", "args": {}}, "final": None},
        {"action": None, "final": "All good."},
    ])
    res = client.post("/agent/message", json={"message": "status"})
    assert res.status_code == 200, res.text
    body = res.json()
    assert body["status"] == "completed" and body["answer"] == "All good."
    assert body["steps_used"] == 1

    memory = client.get(f"/memory/{body['session_id']}").json()
    assert [m["role"] for m in memory["messages"]] == ["user", "tool", "assistant"]


def test_agent_message_requires_text_or_approval():
    assert client.post("/agent/message", json={"message": "  "}).status_code == 400
    assert client.post("/agent/message",
                       json={"approval_token": "g1.a.b"}).status_code == 400


def test_agent_approval_over_http(tmp_path, monkeypatch):
    _scripted_agent(tmp_path, monkeypatch, [
        {"action": {"tool": "service.restart", "args": {"unit": "cron.service"}}, "final": None},
        {"action": None, "final": "cron.service restarted."},
    ])
    paused = client.post("/agent/message", json={"message": "restart cron"}).json()
    assert paused["status"] == "awaiting_approval"
    approval_id = paused["approval"]["approval_id"]

    resumed = client.post("/agent/approve", json={
        "session_id": paused["session_id"], "approval_id": approval_id})
    assert resumed.status_code == 200, resumed.text
    body = resumed.json()
    assert body["status"] == "completed"
    assert body["answer"] == "cron.service restarted."


def test_agent_approve_unknown_id_is_404():
    res = client.post("/agent/approve", json={
        "session_id": "ses_x", "approval_id": "apr_missing"})
    assert res.status_code == 404


def test_mind_status_endpoint():
    body = client.get("/mind/status").json()
    assert body["max_steps"] >= 1
    assert "backends" in body["router"]
    assert "memory" in body


def test_memory_sessions_endpoint():
    body = client.get("/memory/sessions").json()
    assert isinstance(body["sessions"], list)

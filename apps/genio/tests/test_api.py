"""HTTP surface tests: capability gate, approval gate, audit endpoints."""

from __future__ import annotations

from fastapi.testclient import TestClient

from genio_server.daemon import app

client = TestClient(app)


def _session() -> str:
    res = client.post("/caps/session", json={"subject": "pytest-cli"})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def test_health_reports_phase6():
    body = client.get("/health").json()
    assert body["status"] == "ok"
    assert body["phase"] == 6
    assert body["broker"]["tools"] >= 9
    assert body["mind"]["max_steps"] >= 1
    assert body["registry"]["count"] >= 9
    assert "store:extract" in body["registry"]["capabilities"]
    assert "store.delete" in body["policy"]["deny_tools"]


def test_tools_catalogue():
    body = client.get("/tools").json()
    ids = {tool["id"] for tool in body["tools"]}
    assert {"telemetry.snapshot", "file.write", "service.restart",
            "store.list", "store.extract", "store.media_info", "store.mkdir"} <= ids
    for tool in body["tools"]:
        assert tool["cap"] and isinstance(tool["tags"], list)


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


# --------------------------------------------------------------------------- policy API
def test_policy_snapshot_lists_rules_and_matrix():
    body = client.get("/policy").json()
    assert body["rules"] and body["matrix"]
    assert body["private_capability"] == "store:private"
    decisions = {row["tool"]: row["decision"] for row in body["matrix"]}
    assert decisions["store.list"] == "ALLOW"
    assert decisions["store.mkdir"] == "REQUIRE_CONFIRMATION"
    assert decisions["service.restart"] == "REQUIRE_CONFIRMATION"


def test_policy_evaluate_denies_private_path_without_private_cap():
    body = client.post("/policy/evaluate", json={
        "tool": "store.extract", "caps": ["store:extract"],
        "args": {"path": "private/secret.pdf"},
    }).json()
    assert body["decision"] == "DENY"
    assert body["code"] == "private_store_denied"
    assert body["found"] is True


def test_policy_evaluate_allows_public_path():
    body = client.post("/policy/evaluate", json={
        "tool": "store.extract", "caps": ["store:extract"],
        "args": {"path": "public/manual.pdf"},
    }).json()
    assert body["decision"] == "ALLOW"


def test_policy_evaluate_unknown_tool():
    body = client.post("/policy/evaluate", json={"tool": "shell.exec"}).json()
    assert body["decision"] == "DENY"
    assert body["code"] == "unknown_tool"
    assert body["found"] is False


def test_portal_status_is_honest_about_offline_store():
    body = client.get("/portal/status").json()
    assert "base_url" in body and "auth_mode" in body
    assert body["ok"] in (True, False)


# --------------------------------------------------------------------------- denial
def test_deny_removes_pending_approval():
    token = _session()
    res = client.post("/tools/service.restart/invoke",
                      json={"args": {"unit": "cron.service"}},
                      headers={"X-Genio-Cap": token})
    assert res.status_code == 409
    approval_id = res.json()["approval_id"]

    denied = client.post(f"/approvals/{approval_id}/deny",
                         json={"approver": "pytest-human"})
    assert denied.status_code == 200, denied.text
    assert denied.json()["denied"] is True

    remaining = client.get("/approvals").json()["approvals"]
    assert all(item["id"] != approval_id for item in remaining)
    assert client.post(f"/approvals/{approval_id}/grant", json={}).status_code == 404


def test_granted_approval_leaves_the_pending_queue():
    token = _session()
    approval_id = client.post(
        "/tools/service.restart/invoke",
        json={"args": {"unit": "cron.service"}},
        headers={"X-Genio-Cap": token},
    ).json()["approval_id"]

    granted = client.post(f"/approvals/{approval_id}/grant", json={"approver": "pytest"})
    assert granted.status_code == 200
    pending = client.get("/approvals").json()["approvals"]
    assert all(item["id"] != approval_id for item in pending)


def test_deny_unknown_approval_is_404():
    res = client.post("/approvals/apr_doesnotexist/deny", json={})
    assert res.status_code == 404

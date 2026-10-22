"""HTTP surface tests: capability gate, approval gate, audit endpoints."""

from __future__ import annotations

from fastapi.testclient import TestClient

from genio_server.daemon import app

client = TestClient(app)


def _session() -> str:
    res = client.post("/caps/session", json={"subject": "pytest-cli"})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def test_health_reports_phase2():
    body = client.get("/health").json()
    assert body["status"] == "ok"
    assert body["phase"] == 2
    assert body["broker"]["tools"] >= 7


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

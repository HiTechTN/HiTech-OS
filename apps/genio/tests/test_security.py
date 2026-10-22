"""Adversarial tests for the Genio capability broker (Phase 2).

Covered attack/failure cases:
  1. R2 without a human approval token        → refused
  2. Replaying a consumed nonce               → refused (single-use)
  3. Approval bound to different arguments    → refused
  4. Path traversal in file.write             → refused (root escape)
  5. Audit log unwritable                     → R1/R2 frozen, R0 still works
  6. Tampered audit chain                     → detected
  7. Expired / forged / under-scoped tokens   → refused
  8. Dangerous unit names                     → refused by validation
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from genio_server.core import broker as broker_mod  # noqa: E402
from genio_server.core import caps  # noqa: E402
from genio_server.core.broker import Broker, Risk, ToolSpec  # noqa: E402


@pytest.fixture(autouse=True)
def _clean_nonces():
    caps.reset_nonce_cache()
    yield
    caps.reset_nonce_cache()


@pytest.fixture()
def calls():
    return []


@pytest.fixture()
def test_tools(tmp_path: Path, calls):
    def r0(_: dict):
        return {"ok": "read"}

    def r1(args: dict):
        calls.append(("r1", args))
        return {"wrote": args.get("path")}

    def r2(args: dict):
        calls.append(("r2", args))
        return {"restarted": args.get("target")}

    return [
        ToolSpec("t.read", Risk.R0, "test read", "test:read", {}, r0, 100),
        ToolSpec("t.write", Risk.R1, "test write", "test:write",
                 {"path": {"type": "str", "required": True},
                  "content": {"type": "str", "required": True}}, r1, 10),
        ToolSpec("t.r2", Risk.R2, "test privileged", "test:r2",
                 {"target": {"type": "str", "required": True}}, r2, 5),
    ]


@pytest.fixture()
def broker(tmp_path: Path, test_tools) -> Broker:
    return Broker(tmp_path, tools=test_tools)


@pytest.fixture()
def session() -> str:
    return caps.issue("pytest", ["test:read", "test:write", "test:r2"], 600)


@pytest.fixture()
def real_session() -> str:
    """Session for the *default* tool registry (real capability names)."""
    return caps.issue("pytest", ["telemetry:read", "service:read", "store:read",
                                 "fs:write", "service:restart"], 600)


# --------------------------------------------------------------------------- R2
def test_r0_auto_executes(broker, session):
    res = broker.invoke("t.read", {}, session)
    assert res["ok"] is True and res["decision"] == "executed"


def test_r2_without_approval_is_refused(broker, session, calls):
    res = broker.invoke("t.r2", {"target": "cron"}, session)
    assert res["ok"] is False
    assert res["decision"] == "approval_required"
    assert res["approval_id"].startswith("apr_")
    assert calls == [], "handler must not run without approval"


def test_r2_with_human_approval_executes(broker, session, calls):
    first = broker.invoke("t.r2", {"target": "cron"}, session)
    token = broker.approvals.grant(first["approval_id"], "pytest-human")
    res = broker.invoke("t.r2", {"target": "cron"}, session, approval_token=token)
    assert res["ok"] is True and res["decision"] == "executed"
    assert calls == [("r2", {"target": "cron"})]


def test_replayed_approval_nonce_is_refused(broker, session, calls):
    first = broker.invoke("t.r2", {"target": "cron"}, session)
    token = broker.approvals.grant(first["approval_id"], "pytest-human")
    ok = broker.invoke("t.r2", {"target": "cron"}, session, approval_token=token)
    replay = broker.invoke("t.r2", {"target": "cron"}, session, approval_token=token)
    assert ok["ok"] is True
    assert replay["ok"] is False
    assert replay["code"] == "approval_replayed"
    assert len(calls) == 1, "replay must not execute twice"


def test_approval_is_bound_to_arguments(broker, session, calls):
    first = broker.invoke("t.r2", {"target": "cron"}, session)
    token = broker.approvals.grant(first["approval_id"], "pytest-human")
    res = broker.invoke("t.r2", {"target": "sshd"}, session, approval_token=token)
    assert res["ok"] is False and res["code"] == "approval_mismatch"
    assert calls == []


def test_forged_approval_token_is_refused(broker, session):
    res = broker.invoke("t.r2", {"target": "cron"}, session,
                        approval_token="g1.Zm9v.YmFy")
    assert res["ok"] is False
    assert res["code"] in ("approval_invalid",)


# --------------------------------------------------------------------------- tokens
def test_expired_token_is_refused(broker, monkeypatch):
    token = caps.issue("pytest", ["test:read"], ttl_s=60)
    real = time.time
    monkeypatch.setattr(caps.time, "time", lambda: real() + 3600)
    res = broker.invoke("t.read", {}, token)
    assert res["ok"] is False and res["code"] == "tokenexpired"


def test_missing_capability_is_refused(broker):
    weak = caps.issue("pytest", ["test:read"], 600)
    res = broker.invoke("t.write", {"path": "a.txt", "content": "x"}, weak)
    assert res["ok"] is False and res["code"] == "capabilitydenied"


def test_tampered_signature_is_refused(broker):
    token = caps.issue("pytest", ["test:read"], 600)
    head, body, sig = token.split(".")
    tampered = f"{head}.{body}.{'A' * len(sig)}"
    res = broker.invoke("t.read", {}, tampered)
    assert res["ok"] is False and res["code"] == "tokeninvalid"


# --------------------------------------------------------------------------- paths
@pytest.mark.parametrize("evil", [
    "../../etc/passwd",
    "/etc/passwd",
    "a/../../escape.txt",
    "",
    "x" * 300,
])
def test_path_traversal_in_file_write_is_blocked(tmp_path, real_session, evil):
    real = Broker(tmp_path)  # default tool registry → real file.write handler
    res = real.invoke("file.write", {"path": evil, "content": "pwned"}, real_session)
    assert res["ok"] is False, f"traversal accepted: {evil!r}"
    assert res["code"] == "invalid_args"


def test_percent_encoded_traversal_writes_a_literal_name(tmp_path, real_session):
    # "..%2f.." is a literal filename (no URL decoding happens here); it must
    # land inside the root, never escape it.
    real = Broker(tmp_path)
    res = real.invoke("file.write", {"path": "..%2f..%2fescape.txt", "content": "x"},
                      real_session)
    assert res["ok"] is True
    assert Path(res["result"]["path"]).resolve().is_relative_to(
        (tmp_path / "files").resolve())


def test_file_write_stays_inside_root(tmp_path, real_session):
    real = Broker(tmp_path)
    res = real.invoke("file.write", {"path": "notes/ok.txt", "content": "hello"}, real_session)
    assert res["ok"] is True
    written = Path(res["result"]["path"])
    assert written.resolve().is_relative_to((tmp_path / "files").resolve())
    assert written.read_text() == "hello"


# --------------------------------------------------------------------------- audit
def test_unwritable_audit_freezes_r1_r2_but_not_r0(session):
    frozen = Broker(Path("/proc/genio-definitely-not-writable"), tools=[
        ToolSpec("t.read", Risk.R0, "read", "test:read", {}, lambda a: {"ok": 1}, 10),
        ToolSpec("t.write", Risk.R1, "write", "test:write",
                 {"path": {"type": "str", "required": True},
                  "content": {"type": "str", "required": True}}, lambda a: {"ok": 1}, 10),
        ToolSpec("t.r2", Risk.R2, "r2", "test:r2",
                 {"target": {"type": "str", "required": True}}, lambda a: {"ok": 1}, 10),
    ])
    assert frozen.audit.ensure_writable() is False

    blocked_r1 = frozen.invoke("t.write", {"path": "a.txt", "content": "x"}, session)
    blocked_r2 = frozen.invoke("t.r2", {"target": "cron"}, session)
    allowed_r0 = frozen.invoke("t.read", {}, session)

    assert blocked_r1["ok"] is False and blocked_r1["code"] == "audit_unavailable"
    assert blocked_r2["ok"] is False and blocked_r2["code"] == "audit_unavailable"
    assert allowed_r0["ok"] is True, "R0 must remain available for observation"


def test_audit_chain_valid_then_tamper_detected(tmp_path, real_session):
    b = Broker(tmp_path)
    b.invoke("file.write", {"path": "log/a.txt", "content": "one"}, real_session)
    b.invoke("file.write", {"path": "log/b.txt", "content": "two"}, real_session)
    verify = b.audit.verify_chain()
    assert verify["ok"] is True and verify["records"] >= 2

    lines = b.audit.path.read_text().splitlines()
    rec = json.loads(lines[0])
    rec["detail"] = "tampered"
    lines[0] = json.dumps(rec)
    b.audit.path.write_text("\n".join(lines) + "\n")

    tampered = b.audit.verify_chain()
    assert tampered["ok"] is False
    assert "hash mismatch" in tampered["error"] or "broken chain" in tampered["error"]


# --------------------------------------------------------------------------- units
@pytest.mark.parametrize("unit", ["ssh.service", "sshd.service", "genio.service",
                                  "systemd-logind.service", "user@1000.service",
                                  "cron", "cron.service; rm -rf /", "../../bin/sh.service"])
def test_dangerous_units_are_refused(tmp_path, real_session, unit):
    b = Broker(tmp_path)
    res = b.invoke("service.restart", {"unit": unit}, real_session)
    assert res["ok"] is False
    assert res["code"] == "invalid_args"


def test_valid_unit_requires_approval(tmp_path, real_session):
    b = Broker(tmp_path)
    res = b.invoke("service.restart", {"unit": "cron.service"}, real_session)
    assert res["decision"] == "approval_required"


# --------------------------------------------------------------------------- rate
def test_r2_rate_limit(tmp_path, real_session):
    b = Broker(tmp_path)
    codes = [b.invoke("service.restart", {"unit": "cron.service"}, real_session)["code"]
             for _ in range(5)]
    assert codes.count("rate_limited") >= 1

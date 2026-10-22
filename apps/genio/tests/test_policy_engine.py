"""Policy engine: deterministic ALLOW / DENY / REQUIRE_CONFIRMATION."""

from __future__ import annotations

import json

import pytest

from genio_server.core.broker import Broker, Risk, ToolSpec
from genio_server.core.caps import issue
from genio_server.core.policy_engine import Decision, PolicyEngine

ALL_CAPS = {"telemetry:read", "service:read", "service:restart", "fs:write",
            "store:read", "store:extract", "store:write", "media:read",
            "test:read", "test:write", "test:r2"}


def make_tool(tool_id="t.read", risk=Risk.R0, cap="test:read", tags=()):
    return ToolSpec(tool_id, risk, "test tool", cap, {}, lambda a: {"ok": True},
                    60, lambda a: "", None, tags)


def evaluate(tool, caps=None, args=None, subject="agent", engine=None):
    return (engine or PolicyEngine()).evaluate(tool, subject, caps if caps is not None else ALL_CAPS, args)


# --------------------------------------------------------------------------- allow
@pytest.mark.parametrize("tool", [
    make_tool("t.read", Risk.R0, "test:read"),
    make_tool("t.write", Risk.R1, "test:write"),
    make_tool("store.list", Risk.R0, "store:read", ("store",)),
    make_tool("store.extract", Risk.R0, "store:extract", ("store", "documents")),
])
def test_r0_and_r1_are_allowed_with_capability(tool):
    verdict = evaluate(tool)
    assert verdict.decision is Decision.ALLOW
    assert verdict.allowed and not verdict.denied and not verdict.requires_confirmation


def test_r2_requires_confirmation_not_allow():
    verdict = evaluate(make_tool("store.mkdir", Risk.R2, "store:write"), args={"parent": "public", "name": "x"})
    assert verdict.decision is Decision.REQUIRE_CONFIRMATION
    assert verdict.code == "requires_human_approval"
    assert verdict.rule == "P7_r2_confirmation"


def test_wildcard_capability_allows_everything():
    assert evaluate(make_tool(cap="store:private"), caps={"*"}).allowed


# --------------------------------------------------------------------------- deny
def test_missing_capability_denies():
    verdict = evaluate(make_tool("store.extract", Risk.R0, "store:extract"), caps={"store:read"})
    assert verdict.decision is Decision.DENY
    assert verdict.code == "capabilitydenied"
    assert verdict.rule == "P4_missing_capability"


def test_unknown_tool_denies():
    verdict = PolicyEngine().evaluate(None, "agent", ALL_CAPS)
    assert verdict.denied and verdict.code == "unknown_tool"


def test_hard_deny_list_cannot_be_unlocked_by_capabilities():
    tool = make_tool("store.delete", Risk.R2, "store:write")
    verdict = evaluate(tool, caps=ALL_CAPS | {"*"})
    assert verdict.denied and verdict.code == "denied_by_policy"
    assert verdict.rule == "P2_deny_list"


def test_blocked_subject_denies():
    engine = PolicyEngine(deny_subjects={"rogue"})
    verdict = engine.evaluate(make_tool(), "rogue", ALL_CAPS)
    assert verdict.denied and verdict.code == "subject_blocked"


@pytest.mark.parametrize("path", ["private/secret.pdf", "private", "/private/x.docx"])
def test_private_paths_require_private_capability(path):
    tool = make_tool("store.extract", Risk.R0, "store:extract", ("store",))
    verdict = evaluate(tool, caps=ALL_CAPS, args={"path": path})
    assert verdict.denied and verdict.code == "private_store_denied"
    assert verdict.rule == "P5_private_store"

    allowed = evaluate(tool, caps=ALL_CAPS | {"store:private"}, args={"path": path})
    assert allowed.allowed


@pytest.mark.parametrize("parent,name", [("private", "x"), ("", "evil"), ("HCF", "x")])
def test_mkdir_outside_write_roots_is_denied(parent, name):
    tool = make_tool("store.mkdir", Risk.R2, "store:write", ("store",))
    verdict = evaluate(tool, args={"parent": parent, "name": name})
    assert verdict.denied and verdict.code == "write_scope_denied"
    assert verdict.rule == "P6_write_scope"


@pytest.mark.parametrize("parent,name", [("public", "reports"), ("agents/genio", "out"), ("", "public")])
def test_mkdir_inside_write_roots_requires_confirmation(parent, name):
    tool = make_tool("store.mkdir", Risk.R2, "store:write", ("store",))
    verdict = evaluate(tool, args={"parent": parent, "name": name})
    assert verdict.requires_confirmation


# --------------------------------------------------------------------------- properties
def test_decisions_are_deterministic():
    engine = PolicyEngine()
    tool = make_tool("store.extract", Risk.R0, "store:extract")
    first = engine.evaluate(tool, "agent", ALL_CAPS, {"path": "public/x.pdf"})
    for _ in range(100):
        assert engine.evaluate(tool, "agent", ALL_CAPS, {"path": "public/x.pdf"}) == first


def test_decision_serialises_with_rule_and_capability():
    verdict = evaluate(make_tool("store.extract", Risk.R0, "store:extract"))
    data = verdict.as_dict()
    assert data["decision"] == "ALLOW"
    assert data["capability"] == "store:extract"
    assert data["rule"] == "P9_r0_allow"


def test_matrix_covers_every_registered_tool():
    tools = [make_tool("a.read", Risk.R0, "test:read"),
             make_tool("a.write", Risk.R1, "test:write"),
             make_tool("a.r2", Risk.R2, "test:r2")]
    matrix = PolicyEngine().matrix(tools, "agent", {"test:read", "test:write", "test:r2"})
    assert [row["decision"] for row in matrix] == ["ALLOW", "ALLOW", "REQUIRE_CONFIRMATION"]


def test_rules_snapshot_is_complete():
    snapshot = PolicyEngine().rules_snapshot()
    ids = {rule["id"] for rule in snapshot["rules"]}
    assert {"P1_unknown_tool", "P4_missing_capability", "P5_private_store",
            "P7_r2_confirmation"} <= ids
    assert "store.delete" in snapshot["deny_tools"]
    assert snapshot["private_capability"] == "store:private"


# --------------------------------------------------------------------------- enforcement
def test_broker_enforces_policy_before_execution(tmp_path):
    executed = []

    def handler(_):
        executed.append(True)
        return {"ok": True}

    tool = ToolSpec("store.delete", Risk.R0, "delete", "store:write", {}, handler, 60)
    policy = PolicyEngine()
    broker = Broker(tmp_path, tools=[tool], policy=policy)
    token = issue("agent", ["store:write"], 600)

    result = broker.invoke("store.delete", {}, token)

    assert result["ok"] is False
    assert result["code"] == "denied_by_policy"
    assert executed == [], "policy DENY must prevent the handler from running"
    assert broker.audit.seq >= 1, "policy denials must be audited"
    last = json.loads(broker.audit.path.read_text().splitlines()[-1])
    assert last["decision"] == "denied" and "denied_by_policy" in last["detail"]


def test_broker_without_policy_still_enforces_risk(tmp_path):
    tool = ToolSpec("t.r2", Risk.R2, "privileged", "test:r2",
                    {"target": {"type": "str", "required": True}},
                    lambda a: {"ok": True}, 60)
    broker = Broker(tmp_path, tools=[tool])
    token = issue("agent", ["test:r2"], 600)
    assert broker.invoke("t.r2", {"target": "x"}, token)["decision"] == "approval_required"

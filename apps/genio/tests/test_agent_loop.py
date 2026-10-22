"""Agent loop: bounded ReAct, broker delegation, approval pause/resume, degradation."""

from __future__ import annotations

import pytest

from genio_server.core import caps
from genio_server.core.agent_loop import AgentLoop
from genio_server.core.broker import Broker, Risk, ToolSpec
from genio_server.core.memory import MemoryStore
from genio_server.core.model_router import ModelRouter, ScriptedBackend


@pytest.fixture(autouse=True)
def _clean_nonces():
    caps.reset_nonce_cache()
    yield
    caps.reset_nonce_cache()


def make_broker(tmp_path, executed):
    def read(_):
        return {"cpu_percent": 1.0, "services": 68}

    def restart(args):
        executed.append(args)
        return {"restarted": args["target"]}

    return Broker(tmp_path, tools=[
        ToolSpec("telemetry.snapshot", Risk.R0, "Telemetry", "telemetry:read", {}, read, 100),
        ToolSpec("t.read", Risk.R0, "Read the system", "test:read", {}, read, 100),
        ToolSpec("t.r2", Risk.R2, "Restart something", "test:r2",
                 {"target": {"type": "str", "required": True}}, restart, 10),
    ])


def make_loop(tmp_path, plans, max_steps=4, broker=None):
    executed: list[dict] = []
    broker = broker or make_broker(tmp_path, executed)
    backend = ScriptedBackend(plans)
    router = ModelRouter([backend], retries=1)
    memory = MemoryStore(tmp_path / "memory")
    events: list[dict] = []
    loop = AgentLoop(
        broker, router, memory, publish=events.append, max_steps=max_steps,
        telemetry=lambda: {"host": {"hostname": "test"},
                           "cpu": {"percent": 1.0}, "memory": {"percent": 2.0},
                           "services": {"running_count": 3, "failed": []}},
    )
    return loop, broker, backend, memory, events, executed


SESSION_CAPS = ["telemetry:read", "test:read", "test:r2"]


def token(caps_list=SESSION_CAPS) -> str:
    return caps.issue("pytest", caps_list, 600)


# --------------------------------------------------------------------------- happy path
def test_react_step_then_final(tmp_path):
    plans = [
        {"thought": "read the system", "action": {"tool": "t.read", "args": {}}, "final": None},
        {"thought": "summarise", "action": None, "final": "Everything is fine."},
    ]
    loop, broker, backend, memory, events, _ = make_loop(tmp_path, plans)
    result = loop.run("how is the machine?", token())

    assert result["status"] == "completed"
    assert result["answer"] == "Everything is fine."
    assert result["steps_used"] == 1
    assert result["steps"][0]["tool"] == "t.read" and result["steps"][0]["ok"] is True
    assert len(backend.calls) == 2, "one planning call per step"

    history = memory.history(result["session_id"])
    assert [m["role"] for m in history] == ["user", "tool", "assistant"]
    assert memory.tool_results(result["session_id"])[0]["tool"] == "t.read"

    phases = [e["phase"] for e in events if e["type"] == "agent"]
    for expected in ("perceive", "plan", "act", "observation", "final"):
        assert expected in phases, f"missing SSE phase {expected}"


def test_history_is_reused_across_turns(tmp_path):
    plans = [
        {"action": {"tool": "t.read", "args": {}}, "final": None},
        {"action": None, "final": "first answer"},
        {"action": {"tool": "t.read", "args": {}}, "final": None},
        {"action": None, "final": "second answer"},
    ]
    loop, broker, backend, memory, _, _ = make_loop(tmp_path, plans)
    first = loop.run("first", token())
    second = loop.run("second", token(), session_id=first["session_id"])

    assert second["session_id"] == first["session_id"]
    roles = [m["role"] for m in memory.history(second["session_id"])]
    assert roles.count("user") == 2 and roles.count("assistant") == 2
    # the second planning prompt must contain the first exchange
    prompt = str(backend.calls[2])
    assert "first answer" in prompt


# --------------------------------------------------------------------------- bounds
def test_max_steps_is_a_hard_bound(tmp_path):
    plans = [{"action": {"tool": "t.read", "args": {}}, "final": None}] * 20
    loop, broker, backend, memory, events, _ = make_loop(tmp_path, plans, max_steps=3)
    result = loop.run("loop forever", token())

    assert result["status"] == "max_steps"
    assert result["steps_used"] == 3
    assert len(backend.calls) == 3, "the model must not be called more than max_steps"
    assert "step limit reached" in result["answer"]
    assert any(e.get("phase") == "reflect" for e in events)


def test_max_steps_is_clamped_to_at_least_one(tmp_path):
    loop, *_ = make_loop(tmp_path, [], max_steps=0)
    assert loop.max_steps == 1


# --------------------------------------------------------------------------- approvals
def test_r2_pauses_and_resumes_with_approval(tmp_path):
    plans = [
        {"thought": "needs a restart", "action": {"tool": "t.r2", "args": {"target": "cron"}},
         "final": None},
        {"thought": "done", "action": None, "final": "cron restarted."},
    ]
    loop, broker, backend, memory, events, executed = make_loop(tmp_path, plans)
    session_token = token()
    paused = loop.run("restart cron", session_token)

    assert paused["status"] == "awaiting_approval"
    assert paused["approval"]["tool"] == "t.r2"
    approval_id = paused["approval"]["approval_id"]
    assert executed == [], "nothing may run before the human says yes"
    assert memory.get_pending(paused["session_id"])["approval_id"] == approval_id
    assert any(e.get("phase") == "approval_required" for e in events)

    approval_token = broker.approvals.grant(approval_id, "pytest-human")
    resumed = loop.resume(paused["session_id"], approval_token, session_token)

    assert resumed["status"] == "completed"
    assert resumed["answer"] == "cron restarted."
    assert executed == [{"target": "cron"}]
    assert memory.get_pending(paused["session_id"]) is None
    assert resumed["steps_used"] == 1  # the approved action; the next plan was final
    assert "restarted" in str(backend.calls[1])  # the model saw the observation


def test_resume_without_pending_action_is_an_error(tmp_path):
    loop, *_ = make_loop(tmp_path, [])
    result = loop.resume("ses_doesnotexist", "g1.x.y", token())
    assert result["status"] == "error"
    assert result["error"] == "no_pending_action"


def test_resume_with_rejected_token_reports_error(tmp_path):
    plans = [{"action": {"tool": "t.r2", "args": {"target": "cron"}}, "final": None}]
    loop, broker, backend, memory, _, executed = make_loop(tmp_path, plans)
    paused = loop.run("restart cron", token())
    resumed = loop.resume(paused["session_id"], "g1.forged.sig", token())

    assert resumed["status"] == "error"
    assert resumed["error"] == "approval_invalid"
    assert executed == []
    assert memory.get_pending(paused["session_id"]) is None


# --------------------------------------------------------------------------- fail-safes
def test_missing_capability_stops_the_loop_immediately(tmp_path):
    plans = [{"action": {"tool": "t.r2", "args": {"target": "cron"}}, "final": None}]
    loop, broker, backend, memory, _, executed = make_loop(tmp_path, plans)
    result = loop.run("restart cron", token(["test:read"]))

    assert result["status"] == "error"
    assert result["error"] == "capabilitydenied"
    assert result["steps_used"] == 1
    assert len(backend.calls) == 1, "a policy denial must not be retried"
    assert executed == []


def test_unknown_tool_is_an_observation_not_an_execution(tmp_path):
    plans = [
        {"action": {"tool": "rm.rf", "args": {}}, "final": None},
        {"action": None, "final": "I cannot do that."},
    ]
    loop, broker, backend, memory, _, _ = make_loop(tmp_path, plans)
    result = loop.run("delete everything", token())

    assert result["status"] == "completed"
    assert result["steps"][0]["decision"] == "denied"
    assert "not in the catalogue" in result["steps"][0]["observation"]


def test_missing_model_degrades_to_deterministic_plan(tmp_path):
    backend = ScriptedBackend([RuntimeError("ollama down")])
    broker = make_broker(tmp_path, [])
    router = ModelRouter([backend], retries=1)
    memory = MemoryStore(tmp_path / "memory")
    loop = AgentLoop(broker, router, memory, telemetry=lambda: {
        "host": {"hostname": "test"}, "cpu": {"percent": 3.0},
        "memory": {"percent": 4.0}, "services": {"running_count": 5, "failed": []}})

    result = loop.run("status", token())

    assert result["degraded"] is True
    assert result["status"] == "completed"
    assert result["steps"][0]["tool"] == "telemetry.snapshot"
    assert "Here is what I found" in result["answer"]


# --------------------------------------------------------------------------- memory
def test_memory_survives_reopen(tmp_path):
    path = tmp_path / "memory"
    store = MemoryStore(path)
    store.ensure_session("ses_persist", "demo")
    store.append_message("ses_persist", "user", "hello")
    store.append_message("ses_persist", "assistant", "hi")
    store.set_pending("ses_persist", "t.r2", {"target": "cron"}, "apr_1", 1)
    store.close()

    reopened = MemoryStore(path)
    history = reopened.history("ses_persist")
    assert [m["content"] for m in history] == ["hello", "hi"]
    assert reopened.get_pending("ses_persist")["approval_id"] == "apr_1"
    assert reopened.sessions()[0]["messages"] == 2

    reopened.clear_pending("ses_persist")
    assert reopened.get_pending("ses_persist") is None
    reopened.close()


def test_history_budget_truncates_old_messages(tmp_path):
    store = MemoryStore(tmp_path / "memory")
    for i in range(30):
        store.append_message("s", "user", f"message-{i}-" + "x" * 200)
    short = store.history("s", limit=30, max_chars=1200)
    assert 0 < len(short) < 30
    assert short[-1]["content"].startswith("message-29")
    store.close()

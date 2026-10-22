"""Agent loop — Perceive → Plan → Authorize → Act → Observe → Reflect.

The loop is a *bounded* ReAct cycle:

* the model proposes **one** action per step as JSON, chosen from the tool
  catalogue it is shown;
* every action passes through the Phase 2 capability broker (never direct
  execution), so R0/R1/R2 policy, audit and rate limits still apply;
* an R2 action pauses the loop, persists the pending intent, and returns
  ``awaiting_approval`` — execution resumes only with a single-use approval
  token bound to that exact tool + arguments;
* ``max_steps`` is a hard bound, and any unrecoverable denial (bad token,
  missing capability, frozen audit) stops the loop instead of retrying;
* when no model is available the loop degrades to a deterministic planner and
  says so (``degraded: true``) rather than pretending to think.
"""

from __future__ import annotations

import json
import logging
import os
import secrets
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Callable

try:  # package mode
    from . import memory as memory_mod, model_router, system_monitor
    from .broker import Broker
except ImportError:  # script mode
    from core import memory as memory_mod, model_router, system_monitor  # type: ignore
    from core.broker import Broker  # type: ignore

log = logging.getLogger("genio.agent")

DEFAULT_MAX_STEPS = int(os.getenv("GENIO_MAX_STEPS", "6"))
FATAL_CODES = {"tokeninvalid", "tokenexpired", "capabilitydenied",
               "audit_unavailable", "replaydetected"}

SYSTEM_PROMPT = """You are Genio, the system agent of HiTech-OS. You perceive the machine and act ONLY by calling tools through a capability broker.

Reply with exactly ONE JSON object:
{"thought": "<one short sentence>", "action": {"tool": "<tool id>", "args": {...}} | null, "final": "<answer for the user>" | null}

Rules:
- Pick at most one tool per turn. The system executes it and shows you the observation.
- Use only tool ids from the catalogue. Never invent ids or arguments.
- Prefer observation over assumption. Never claim an action you did not call.
- When you have enough information, set action to null and write the answer in "final".
- R2 tools are privileged: the system pauses for a human approval; that is normal.
- Be concise. The operator sees your "thought" in the event stream."""

STATUS_WORDS = ("status", "état", "etat", "santé", "sante", "health", "how is the machine",
                "cpu", "memory", "ram", "load")


@dataclass
class AgentStep:
    step: int
    thought: str = ""
    tool: str | None = None
    args: dict[str, Any] = field(default_factory=dict)
    decision: str | None = None
    ok: bool = False
    observation: Any = None
    audit_seq: int | None = None

    def as_dict(self) -> dict[str, Any]:
        obs = self.observation
        if not isinstance(obs, str):
            obs = json.dumps(obs, ensure_ascii=False, default=str)
        return {"step": self.step, "thought": self.thought, "tool": self.tool,
                "args": self.args, "decision": self.decision, "ok": self.ok,
                "audit_seq": self.audit_seq, "observation": obs[:1500]}


class AgentLoop:
    def __init__(self, broker: Broker, router: model_router.ModelRouter,
                 memory: memory_mod.MemoryStore,
                 publish: Callable[[dict[str, Any]], None] | None = None,
                 telemetry: Callable[..., dict[str, Any]] | None = None,
                 max_steps: int = DEFAULT_MAX_STEPS, history_limit: int = 16) -> None:
        self.broker = broker
        self.router = router
        self.memory = memory
        self.publish = publish or (lambda event: None)
        self.telemetry = telemetry or system_monitor.snapshot
        self.max_steps = max(1, int(max_steps))
        self.history_limit = history_limit

    # ------------------------------------------------------------------ public
    def run(self, message: str, session_token: str,
            session_id: str | None = None) -> dict[str, Any]:
        if not message or not message.strip():
            raise ValueError("message must not be empty")
        session_id = session_id or f"ses_{secrets.token_hex(8)}"
        self.memory.ensure_session(session_id, title=message.strip()[:120])
        self.memory.append_message(session_id, "user", message.strip())

        self._emit("perceive", session_id=session_id, step=0,
                   detail="collecting system state")
        brief = self._state_brief(self._snapshot())
        self._emit("thought", session_id=session_id, step=0,
                   detail=f"session {session_id} · max_steps={self.max_steps}")
        return self._drive(session_id, session_token, brief, [])

    def resume(self, session_id: str, approval_token: str,
               session_token: str) -> dict[str, Any]:
        """Continue a paused loop after the human granted approval."""
        pending = self.memory.get_pending(session_id)
        if not pending:
            return self._result("error", "No pending action for this session.",
                                session_id, [], error="no_pending_action")
        tool, args = pending["tool"], pending["args"] or {}
        step_no = int(pending.get("step") or 1)
        self._emit("act", session_id=session_id, step=step_no, tool=tool, args=args,
                   detail="resuming with human approval")
        result = self.broker.invoke(tool, args, session_token, approval_token)
        self.memory.clear_pending(session_id)

        ok = bool(result.get("ok"))
        decision = result.get("decision")
        observation = result.get("result") if ok else {
            "code": result.get("code"), "message": result.get("message")}
        step = AgentStep(step_no, "Human approved the pending action.", tool, args,
                         decision, ok, observation, result.get("audit_seq"))
        self.memory.record_tool_result(session_id, tool, args, decision or "unknown", ok, result)
        self._emit("observation", session_id=session_id, step=step_no, tool=tool,
                   ok=ok, decision=decision, summary=self._text(tool, result)[:300])

        if not ok:
            answer = (f"The approved action could not be completed: "
                      f"{result.get('code')} — {result.get('message')}")
            self.memory.append_message(session_id, "assistant", answer)
            self._emit("final", session_id=session_id, answer=answer, degraded=False)
            return self._result("error", answer, session_id, [step],
                                error=result.get("code"))

        self.memory.append_message(session_id, "tool", self._text(tool, result),
                                   meta={"tool": tool, "decision": decision})
        brief = self._state_brief(self._snapshot())
        return self._drive(session_id, session_token, brief, [step])

    # ------------------------------------------------------------------ loop
    def _drive(self, session_id: str, session_token: str, brief: str,
               steps: list[AgentStep]) -> dict[str, Any]:
        degraded = False
        answer: str | None = None
        context = self._context(session_id)
        tool_ids = {t["id"] for t in self.broker.tools()}
        status = "completed"

        while len(steps) < self.max_steps:
            step_no = len(steps) + 1
            self._emit("plan", session_id=session_id, step=step_no,
                       detail=f"step {step_no}/{self.max_steps}")

            plan, meta = self._plan(session_id, brief, context,
                                    allow_fallback=not steps)
            degraded = degraded or bool(meta.get("degraded"))
            if plan.get("_stop"):
                answer = self._summarize(
                    steps, note=f"model unavailable: {plan.get('_error')}")
                self._emit("reflect", session_id=session_id, step=step_no,
                           detail="model unavailable — summarising observations")
                status = "completed"
                break
            thought = str(plan.get("thought") or "")[:600]
            action = plan.get("action") if isinstance(plan.get("action"), dict) else None
            self._emit("plan", session_id=session_id, step=step_no,
                       thought=thought, action=action, final=plan.get("final"),
                       **{k: meta[k] for k in ("backend", "latency_ms") if k in meta})

            if not action:
                answer = str(plan.get("final") or thought or "No action required.")
                self._emit("reflect", session_id=session_id, step=step_no,
                           detail="goal satisfied")
                break

            tool = str(action.get("tool") or "")
            args = action.get("args") if isinstance(action.get("args"), dict) else {}
            if tool not in tool_ids:
                observation = {"code": "unknown_tool",
                               "message": f"tool {tool!r} is not in the catalogue"}
                steps.append(AgentStep(step_no, thought, tool, args, "denied", False, observation))
                context.append({"role": "user",
                                "content": f"Observation: {json.dumps(observation)}. "
                                           f"Choose a valid tool id or answer."})
                self._emit("observation", session_id=session_id, step=step_no,
                           tool=tool, ok=False, decision="denied",
                           summary=observation["message"])
                continue

            self._emit("act", session_id=session_id, step=step_no, tool=tool, args=args)
            result = self.broker.invoke(tool, args, session_token)
            ok = bool(result.get("ok"))
            decision = result.get("decision")
            observation = result.get("result") if ok else {
                "code": result.get("code"), "message": result.get("message")}
            step = AgentStep(step_no, thought, tool, args, decision, ok,
                             observation, result.get("audit_seq"))
            steps.append(step)
            self.memory.record_tool_result(session_id, tool, args,
                                           decision or "unknown", ok, result)
            self._emit("observation", session_id=session_id, step=step_no, tool=tool,
                       ok=ok, decision=decision, summary=self._text(tool, result)[:300])

            if decision == "approval_required":
                self.memory.set_pending(session_id, tool, args,
                                        result.get("approval_id"), step_no)
                self.memory.append_message(
                    session_id, "assistant",
                    f"Approval required for {tool} {json.dumps(args, ensure_ascii=False)}",
                    meta={"approval_id": result.get("approval_id")})
                approval = {
                    "approval_id": result.get("approval_id"),
                    "tool": tool, "args": args, "risk": result.get("risk"),
                    "prompt": result.get("approval_prompt"),
                    "expires_in_s": result.get("expires_in_s"),
                }
                self._emit("approval_required", session_id=session_id, step=step_no,
                           **approval)
                return self._result(
                    "awaiting_approval",
                    f"Paused: {tool} needs human approval. Approve or deny and I will continue.",
                    session_id, steps, degraded=degraded, approval=approval)

            if not ok and result.get("code") in FATAL_CODES:
                answer = (f"Stopped for safety: {result.get('code')} — "
                          f"{result.get('message')}")
                self.memory.append_message(session_id, "assistant", answer)
                self._emit("error", session_id=session_id, step=step_no,
                           code=result.get("code"), detail=result.get("message"))
                return self._result("error", answer, session_id, steps,
                                    degraded=degraded, error=result.get("code"))

            self.memory.append_message(session_id, "tool", self._text(tool, result),
                                       meta={"tool": tool, "decision": decision})
            context.append({"role": "user",
                            "content": f"Observation from {tool}: "
                                       f"{json.dumps(observation, ensure_ascii=False, default=str)[:2000]}"})
        else:
            status = "max_steps"
            answer = self._summarize(steps, note="step limit reached")
            self._emit("reflect", session_id=session_id, step=len(steps),
                       detail="max_steps reached")

        if answer is None:
            answer = self._summarize(steps)
        self.memory.append_message(session_id, "assistant", answer)
        self._emit("final", session_id=session_id, answer=answer[:1200], degraded=degraded)
        return self._result(status, answer, session_id, steps, degraded=degraded)

    # ------------------------------------------------------------------ planning
    def _plan(self, session_id: str, brief: str, context: list[dict[str, str]],
              allow_fallback: bool = True) -> tuple[dict[str, Any], dict[str, Any]]:
        messages = self._messages(brief, context)
        try:
            plan, response = self.router.complete_json(messages, max_tokens=320)
            return plan, {"backend": response.backend, "latency_ms": response.latency_ms,
                          "attempts": response.attempts}
        except (model_router.ModelUnavailable, model_router.JSONParseError,
                model_router.RouterError) as exc:
            log.warning("planner degraded: %s", exc)
            self._emit("plan", session_id=session_id, detail=f"degraded: {exc}")
            if not allow_fallback:
                # We already observed something; do not spin on a dead model.
                return {"_stop": True, "_error": str(exc)}, {
                    "degraded": True, "backend": "deterministic", "latency_ms": 0.0}
            return self._fallback_plan(session_id, brief, reason=str(exc)), {
                "degraded": True, "backend": "deterministic", "latency_ms": 0.0}

    def _fallback_plan(self, session_id: str, brief: str, reason: str) -> dict[str, Any]:
        last = next((m["content"] for m in reversed(self.memory.history(session_id, limit=6))
                     if m["role"] == "user"), "")
        low = last.lower()
        ids = {t["id"] for t in self.broker.tools()}
        note = f"Model unavailable ({reason.split(':')[0]}); deterministic mode."

        if "telemetry.snapshot" in ids and any(w in low for w in STATUS_WORDS):
            return {"thought": "Fallback: collect telemetry.", "final": None,
                    "action": {"tool": "telemetry.snapshot", "args": {}},
                    "_degraded_note": note}
        if "services.list" in ids and "service" in low:
            return {"thought": "Fallback: list services.", "final": None,
                    "action": {"tool": "services.list", "args": {}}, "_degraded_note": note}
        if "store.extract" in ids and low.startswith("store "):
            return {"thought": "Fallback: extract document.", "final": None,
                    "action": {"tool": "store.extract", "args": {"path": last[6:].strip()}},
                    "_degraded_note": note}
        catalogue = ", ".join(sorted(ids)) or "(none)"
        return {"thought": "Fallback: answer without tools.", "action": None,
                "final": f"{note} I can observe and act through: {catalogue}. "
                         f"Current state — {brief[:600]}"}

    # ------------------------------------------------------------------ prompts
    def _messages(self, brief: str, context: list[dict[str, str]]) -> list[dict[str, str]]:
        system = (f"{SYSTEM_PROMPT}\n\nCURRENT SYSTEM STATE\n{brief}\n\n"
                  f"TOOL CATALOGUE\n{self._catalogue()}")
        return [{"role": "system", "content": system}, *context[-self.history_limit:]]

    def _catalogue(self) -> str:
        lines = []
        for tool in self.broker.tools():
            params = ", ".join(f"{k}:{v.get('type', 'any')}" for k, v in tool["params"].items())
            lines.append(f"- {tool['id']} [{tool['risk']}] {tool['description']} "
                         f"(args: {params or 'none'})")
        return "\n".join(lines)

    def _context(self, session_id: str) -> list[dict[str, str]]:
        context: list[dict[str, str]] = []
        for item in self.memory.history(session_id, limit=self.history_limit):
            role = item["role"]
            if role == "assistant":
                context.append({"role": "assistant", "content": item["content"]})
            elif role == "tool":
                context.append({"role": "user", "content": f"[tool result] {item['content']}"})
            elif role == "user":
                context.append({"role": "user", "content": item["content"]})
        return context

    def _state_brief(self, snapshot: dict[str, Any]) -> str:
        cpu = (snapshot.get("cpu") or {}).get("percent")
        mem = (snapshot.get("memory") or {}).get("percent")
        svc = snapshot.get("services") or {}
        host = snapshot.get("host") or {}
        failed = svc.get("failed") or svc.get("failed_units") or []
        return (f"host={host.get('hostname', '?')} cpu={cpu}% mem={mem}% "
                f"services_running={svc.get('running_count')} "
                f"failed={', '.join(failed[:5]) if failed else 'none'}")

    # ------------------------------------------------------------------ helpers
    def _snapshot(self) -> dict[str, Any]:
        try:
            return self.telemetry()
        except Exception as exc:  # telemetry must never break the loop
            log.warning("telemetry unavailable: %s", exc)
            return {}

    def _text(self, tool: str, result: dict[str, Any]) -> str:
        if result.get("ok"):
            body = json.dumps(result.get("result"), ensure_ascii=False, default=str)
        else:
            body = f"{result.get('code')}: {result.get('message')}"
        return f"{tool}: {body[:2500]}"

    def _summarize(self, steps: list[AgentStep], note: str | None = None) -> str:
        if not steps:
            return "I could not complete that request." + (f" ({note})" if note else "")
        lines = ["Here is what I found:"]
        for step in steps:
            if step.ok:
                body = json.dumps(step.observation, ensure_ascii=False, default=str)
                lines.append(f"- {step.tool} → {body[:220]}")
            else:
                code = (step.observation or {}).get("code") if isinstance(step.observation, dict) else "denied"
                lines.append(f"- {step.tool} → refused ({code})")
        if note:
            lines.append(f"({note})")
        return "\n".join(lines)[:4000]

    def _emit(self, phase: str, **fields: Any) -> None:
        event = {"type": "agent", "phase": phase,
                 "ts": datetime.now(timezone.utc).isoformat(timespec="seconds"), **fields}
        event = {k: v for k, v in event.items() if v is not None}
        try:
            self.publish(event)
        except Exception as exc:  # events are best-effort observability
            log.debug("publish failed: %s", exc)

    def _result(self, status: str, answer: str, session_id: str,
                steps: list[AgentStep], *, degraded: bool = False,
                approval: dict[str, Any] | None = None,
                error: str | None = None) -> dict[str, Any]:
        return {
            "status": status, "answer": answer, "session_id": session_id,
            "steps": [s.as_dict() for s in steps], "steps_used": len(steps),
            "max_steps": self.max_steps, "degraded": degraded,
            "approval": approval, "error": error,
            "router": self.router.status(),
        }

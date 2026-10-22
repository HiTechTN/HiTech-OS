#!/usr/bin/env python3
"""Genio daemon — core API, Agent Loop and Capability Broker (Phase 2).

Endpoints
    GET  /health                 liveness, loop state, degraded flag
    GET  /telemetry              system snapshot (read-only)
    GET  /agent/status           tick counter, ring buffer
    POST /caps/session           local session token (loopback only, dev bootstrap)
    GET  /tools                  tool catalogue with risk classes
    POST /tools/{id}/invoke      execute through the broker (capability required)
    GET  /approvals              pending R2 requests
    POST /approvals/{id}/grant   human decision → single-use approval token
    GET  /audit/verify           hash-chain verification
    GET  /audit/tail             last N audit records
    POST /agent/message          chat/task intake (deterministic router)
    GET  /agent/events           SSE stream (ticks + tool events)

The daemon never executes a tool directly: everything goes through
``core.broker.Broker`` which enforces capabilities, risk classes, approvals,
rate limits and the chained audit log.
"""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import json
import logging
import os
import signal
import sys
import time
from collections import deque
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException, Query, Request
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel, Field

try:  # package mode (uvicorn genio_server.daemon:app)
    from .core import broker as broker_mod
    from .core import caps, system_monitor
except ImportError:  # script mode (cd genio_server && uvicorn daemon:app)
    from core import broker as broker_mod  # type: ignore[no-redef]
    from core import caps, system_monitor  # type: ignore[no-redef]

VERSION = "0.2.0-phase2"
HOST = os.getenv("GENIO_HOST", "127.0.0.1")
PORT = int(os.getenv("GENIO_PORT", "8765"))
TICK_SECONDS = float(os.getenv("GENIO_TICK_SECONDS", "5"))
RING_SIZE = int(os.getenv("GENIO_RING_SIZE", "60"))
LOG_LEVEL = os.getenv("GENIO_LOG_LEVEL", "INFO").upper()
STATE_DIR = Path(os.getenv("GENIO_STATE_DIR", "/var/lib/genio"))
SESSION_TTL_S = int(os.getenv("GENIO_SESSION_TTL", str(12 * 3600)))

# Caps a local client may obtain. R2 execution still needs a human approval:
# the capability only authorises the *attempt*.
DEFAULT_SESSION_CAPS = [
    "telemetry:read", "service:read", "store:read", "fs:write", "service:restart",
]

logging.basicConfig(level=LOG_LEVEL, format="%(asctime)s [genio] %(levelname)s: %(message)s")
log = logging.getLogger("genio.daemon")

BROKER = broker_mod.build_default_broker(STATE_DIR)

STATE: dict[str, Any] = {
    "started_at": time.time(), "ticks": 0, "errors": 0, "last_tick": None,
    "last_tick_ms": None, "last_snapshot": None, "loop_running": False,
    "degraded": False,
}
RING: deque[dict[str, Any]] = deque(maxlen=RING_SIZE)
SUBSCRIBERS: set[asyncio.Queue] = set()


def _iso(ts: float | None = None) -> str:
    return datetime.fromtimestamp(ts or time.time(), tz=timezone.utc).isoformat(timespec="seconds")


def publish(event: dict[str, Any]) -> None:
    """Fan out an event to SSE subscribers (never blocks, never raises)."""
    for queue in list(SUBSCRIBERS):
        try:
            queue.put_nowait(event)
        except asyncio.QueueFull:
            SUBSCRIBERS.discard(queue)


async def agent_loop() -> None:
    """Perceive tick. Phase 3 inserts plan→authorize→act on top of this."""
    STATE["loop_running"] = True
    log.info("agent loop started (tick=%.1fs)", TICK_SECONDS)
    try:
        while True:
            t0 = time.perf_counter()
            try:
                snap = await asyncio.to_thread(system_monitor.snapshot)
                elapsed_ms = round((time.perf_counter() - t0) * 1000, 2)
                STATE["ticks"] += 1
                STATE["last_tick"] = _iso()
                STATE["last_tick_ms"] = elapsed_ms
                STATE["last_snapshot"] = snap
                STATE["degraded"] = bool(snap.get("error"))
                cpu = (snap.get("cpu") or {}).get("percent")
                mem = (snap.get("memory") or {}).get("percent")
                svc = (snap.get("services") or {}).get("running_count")
                sample = {"ts": STATE["last_tick"], "cpu_percent": cpu,
                          "mem_percent": mem, "services_running": svc,
                          "collector_ms": snap.get("collector_ms")}
                RING.append(sample)
                publish({"type": "tick", **sample})
                log.info("tick %d: cpu=%s%% mem=%s%% services=%s (%s ms)",
                         STATE["ticks"], cpu, mem, svc, elapsed_ms)
            except asyncio.CancelledError:
                raise
            except Exception:
                STATE["errors"] += 1
                STATE["degraded"] = True
                STATE["last_tick"] = _iso()
                log.exception("agent tick failed")
            await asyncio.sleep(TICK_SECONDS)
    finally:
        STATE["loop_running"] = False
        log.info("agent loop stopped")


@contextlib.asynccontextmanager
async def lifespan(_: FastAPI):
    task = asyncio.create_task(agent_loop(), name="genio-agent-loop")
    try:
        yield
    finally:
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await task


app = FastAPI(
    title="Genio Daemon",
    description="Sovereign system-aware agent — core API (Phase 2)",
    version=VERSION,
    lifespan=lifespan,
)


# --------------------------------------------------------------------------- models
class SessionBody(BaseModel):
    subject: str = Field("cli", max_length=64)
    caps: list[str] | None = None


class InvokeBody(BaseModel):
    args: dict[str, Any] = Field(default_factory=dict)
    approval_token: str | None = None


class GrantBody(BaseModel):
    approver: str = Field("local-human", max_length=64)


class MessageBody(BaseModel):
    message: str = Field(..., min_length=1, max_length=4000)


# --------------------------------------------------------------------------- health
@app.get("/health")
async def health() -> dict[str, Any]:
    return {
        "status": "ok", "service": "genio-daemon", "version": VERSION, "phase": 2,
        "uptime_s": round(time.time() - STATE["started_at"], 1),
        "agent": {
            "loop_running": STATE["loop_running"], "ticks": STATE["ticks"],
            "errors": STATE["errors"], "last_tick": STATE["last_tick"],
            "tick_seconds": TICK_SECONDS, "degraded": STATE["degraded"],
        },
        "broker": {
            "tools": len(BROKER.tools()),
            "audit_writable": BROKER.audit.ensure_writable(),
            "audit_seq": BROKER.audit.seq,
            "pending_approvals": len(BROKER.approvals.list()),
            "state_dir": str(STATE_DIR),
        },
        "caps": {"key_source": caps.key_source()},
    }


@app.get("/telemetry")
async def telemetry(fresh: bool = Query(False, description="bypass the TTL cache")) -> JSONResponse:
    return JSONResponse(await asyncio.to_thread(system_monitor.snapshot, fresh))


@app.get("/agent/status")
async def agent_status() -> dict[str, Any]:
    return {
        "version": VERSION, "ticks": STATE["ticks"], "errors": STATE["errors"],
        "loop_running": STATE["loop_running"], "last_tick": STATE["last_tick"],
        "last_tick_ms": STATE["last_tick_ms"], "degraded": STATE["degraded"],
        "tick_seconds": TICK_SECONDS, "ring": list(RING),
        "pending_approvals": BROKER.approvals.list(),
    }


# --------------------------------------------------------------------------- caps
@app.post("/caps/session")
async def issue_session(request: Request, body: SessionBody | None = None) -> dict[str, Any]:
    """Bootstrap a local session token.

    Loopback-only and intentionally narrow: Phase 3 replaces this with the
    admin OTP flow. A session may *attempt* R2 actions but cannot execute them
    without a separate, args-bound approval token.
    """
    host = (request.client.host if request.client else "") or ""
    if host not in ("127.0.0.1", "::1", "localhost", "testclient"):
        raise HTTPException(status_code=403, detail="session issuance is loopback-only")
    payload = body or SessionBody()
    requested = payload.caps or DEFAULT_SESSION_CAPS
    granted = [c for c in requested if c in DEFAULT_SESSION_CAPS]
    token = caps.issue(payload.subject, granted, ttl_s=SESSION_TTL_S,
                       kind="session", reusable=True)
    log.info("session issued to %r (caps=%s)", payload.subject, granted)
    return {"token": token, "caps": granted, "expires_in_s": SESSION_TTL_S}


# --------------------------------------------------------------------------- tools
@app.get("/tools")
async def list_tools() -> dict[str, Any]:
    tools = BROKER.tools()
    return {"count": len(tools), "tools": tools}


@app.post("/tools/{tool_id}/invoke")
async def invoke_tool(tool_id: str, body: InvokeBody, request: Request) -> JSONResponse:
    token = request.headers.get("X-Genio-Cap", "").strip()
    if not token:
        raise HTTPException(status_code=401, detail="missing X-Genio-Cap session token")
    result = await asyncio.to_thread(
        BROKER.invoke, tool_id, body.args, token, body.approval_token
    )
    publish({"type": "tool", "tool": tool_id, "ok": bool(result.get("ok")),
             "decision": result.get("decision"), "risk": result.get("risk")})
    status = 200
    if not result.get("ok"):
        status = 409 if result.get("decision") == "approval_required" else 403
        if result.get("decision") == "failed":
            status = 500
    return JSONResponse(result, status_code=status)


# --------------------------------------------------------------------------- approvals
@app.get("/approvals")
async def list_approvals() -> dict[str, Any]:
    items = BROKER.approvals.list()
    return {"count": len(items), "approvals": items}


@app.post("/approvals/{approval_id}/grant")
async def grant_approval(approval_id: str, body: GrantBody | None = None) -> dict[str, Any]:
    approver = (body or GrantBody()).approver
    try:
        token = BROKER.approvals.grant(approval_id, approver)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc.args[0])) from exc
    pending = BROKER.approvals.get(approval_id)
    log.warning("R2 approval granted by %r for %s", approver,
                pending.tool if pending else approval_id)
    publish({"type": "approval", "approval_id": approval_id, "approver": approver,
             "tool": pending.tool if pending else None})
    return {"ok": True, "approval_token": token, "expires_in_s": 120}


# --------------------------------------------------------------------------- audit
@app.get("/audit/verify")
async def audit_verify() -> dict[str, Any]:
    return {**BROKER.audit.verify_chain(), "head": BROKER.audit.head,
            "seq": BROKER.audit.seq, "path": str(BROKER.audit.path)}


@app.get("/audit/tail")
async def audit_tail(n: int = Query(20, ge=1, le=500)) -> dict[str, Any]:
    path = BROKER.audit.path
    try:
        lines = path.read_text(encoding="utf-8").splitlines()[-n:]
    except OSError:
        return {"count": 0, "records": []}
    records = []
    for line in lines:
        with contextlib.suppress(json.JSONDecodeError):
            records.append(json.loads(line))
    return {"count": len(records), "records": records}


# --------------------------------------------------------------------------- agent
def _route_message(message: str) -> dict[str, Any]:
    """Deterministic Phase 2 router. The LLM planner lands in Phase 3.

    It never executes a tool: it answers or *suggests* a tool call, which the
    client must then invoke through the broker (agent proposes, broker disposes).
    """
    text = message.strip()
    low = text.lower()
    snap = STATE["last_snapshot"] or {}
    cpu = (snap.get("cpu") or {}).get("percent")
    mem = (snap.get("memory") or {}).get("percent")
    svc = (snap.get("services") or {}).get("running_count")
    failed = (snap.get("services") or {}).get("failed_units") or []

    if low in ("help", "/help", "?"):
        return {"reply": "Commands: status · services · tools · audit · store <path> · "
                         "or invoke a tool with /call. Say 'status' for a snapshot."}
    if any(k in low for k in ("status", "état", "etat", "santé", "health")):
        return {"reply": f"System: CPU {cpu}% · RAM {mem}% · {svc} services running"
                         + (f" · FAILED: {', '.join(failed[:5])}" if failed else " · no failed units")
                         + f" · loop ticks {STATE['ticks']} (errors {STATE['errors']}).",
                "suggestion": {"tool": "telemetry.snapshot", "args": {}}}
    if "service" in low:
        return {"reply": "Use the services.list tool (R0) for the full inventory; "
                         "restarting a unit is R2 and needs your explicit approval.",
                "suggestion": {"tool": "services.list", "args": {}}}
    if "tool" in low:
        return {"reply": f"{len(BROKER.tools())} tools registered. R0 auto, R1 auto+audited, "
                         "R2 requires a signed approval token.",
                "suggestion": None}
    if "audit" in low:
        chain = BROKER.audit.verify_chain()
        return {"reply": f"Audit chain: {'OK' if chain['ok'] else 'BROKEN'} · "
                         f"{chain.get('records', 0)} records · seq {BROKER.audit.seq}.",
                "suggestion": None}
    if low.startswith("store "):
        path = text[6:].strip()
        return {"reply": f"Document extraction for {path!r} is R0; invoking store.extract.",
                "suggestion": {"tool": "store.extract", "args": {"path": path}}}
    return {"reply": "Phase 2 router active (no LLM yet). Try: status · services · tools · "
                     "audit · store <path>. Privileged actions need approval.",
            "suggestion": None}


@app.post("/agent/message")
async def agent_message(body: MessageBody) -> dict[str, Any]:
    result = _route_message(body.message)
    publish({"type": "message", "message": body.message[:200],
             "reply": result["reply"][:300]})
    log.info("agent message handled (%d chars)", len(body.message))
    return {"ok": True, "ts": _iso(), **result}


@app.get("/agent/events")
async def agent_events(request: Request) -> StreamingResponse:
    queue: asyncio.Queue = asyncio.Queue(maxsize=200)
    SUBSCRIBERS.add(queue)

    async def stream():
        try:
            yield f"event: hello\ndata: {json.dumps({'version': VERSION, 'ts': _iso()})}\n\n"
            while True:
                if await request.is_disconnected():
                    break
                try:
                    event = await asyncio.wait_for(queue.get(), timeout=15)
                    yield f"event: {event.get('type', 'message')}\ndata: {json.dumps(event, ensure_ascii=False)}\n\n"
                except asyncio.TimeoutError:
                    yield ": keep-alive\n\n"
        finally:
            SUBSCRIBERS.discard(queue)

    return StreamingResponse(stream(), media_type="text/event-stream",
                             headers={"Cache-Control": "no-cache",
                                      "X-Accel-Buffering": "no"})


# --------------------------------------------------------------------------- main
def _shutdown(*_: Any) -> None:
    log.info("shutdown signal received")
    raise SystemExit(0)


def main() -> int:
    parser = argparse.ArgumentParser(description="Genio daemon (Phase 2)")
    parser.add_argument("--snapshot", action="store_true",
                        help="print one telemetry snapshot and exit")
    parser.add_argument("--host", default=HOST)
    parser.add_argument("--port", type=int, default=PORT)
    args = parser.parse_args()

    if args.snapshot:
        print(json.dumps(system_monitor.snapshot(force=True), indent=2, ensure_ascii=False))
        return 0

    import uvicorn

    signal.signal(signal.SIGTERM, _shutdown)
    log.info("starting Genio daemon v%s on http://%s:%d (state=%s)",
             VERSION, args.host, args.port, STATE_DIR)
    uvicorn.run(app, host=args.host, port=args.port, log_level=LOG_LEVEL.lower())
    return 0


if __name__ == "__main__":
    sys.exit(main())

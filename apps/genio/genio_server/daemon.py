#!/usr/bin/env python3
"""Genio daemon — core API + nascent Agent Loop (Phase 1).

The daemon is the only privileged-ish component of Genio: CLI and Web are thin
clients. Phase 1 ships the skeleton:

  * /health        liveness, version, uptime, loop state
  * /telemetry     latest system snapshot (read-only probes)
  * /agent/status  tick counter, errors, ring buffer
  * background     Agent Loop "perceive" tick (plan/act arrive in Phase 2)

Run:
    uvicorn daemon:app --host 127.0.0.1 --port 8765
    python daemon.py --snapshot        # one-shot telemetry probe, no server
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
from typing import Any

from fastapi import FastAPI, Query
from fastapi.responses import JSONResponse

try:  # package mode (uvicorn genio_server.daemon:app)
    from .core import system_monitor
except ImportError:  # script mode (uvicorn daemon:app from genio_server/)
    from core import system_monitor  # type: ignore[no-redef]

VERSION = "0.1.0-phase1"
HOST = os.getenv("GENIO_HOST", "127.0.0.1")
PORT = int(os.getenv("GENIO_PORT", "8765"))
TICK_SECONDS = float(os.getenv("GENIO_TICK_SECONDS", "5"))
RING_SIZE = int(os.getenv("GENIO_RING_SIZE", "60"))
LOG_LEVEL = os.getenv("GENIO_LOG_LEVEL", "INFO").upper()

logging.basicConfig(
    level=LOG_LEVEL,
    format="%(asctime)s [genio] %(levelname)s: %(message)s",
)
log = logging.getLogger("genio.daemon")

# --------------------------------------------------------------------------- state
STATE: dict[str, Any] = {
    "started_at": time.time(),
    "ticks": 0,
    "errors": 0,
    "last_tick": None,
    "last_tick_ms": None,
    "last_snapshot": None,
    "loop_running": False,
    "degraded": False,
}
RING: deque[dict[str, Any]] = deque(maxlen=RING_SIZE)


def _iso(ts: float | None = None) -> str:
    return datetime.fromtimestamp(ts or time.time(), tz=timezone.utc).isoformat(timespec="seconds")


async def agent_loop() -> None:
    """Minimal 'perceive' loop. Phase 2 inserts plan→authorize→act here."""
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
                RING.append({
                    "ts": STATE["last_tick"],
                    "cpu_percent": cpu,
                    "mem_percent": mem,
                    "services_running": svc,
                    "collector_ms": snap.get("collector_ms"),
                })
                log.info(
                    "tick %d: cpu=%s%% mem=%s%% services=%s (%s ms)",
                    STATE["ticks"], cpu, mem, svc, elapsed_ms,
                )
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
    description="Sovereign system-aware agent — core API (Phase 1)",
    version=VERSION,
    lifespan=lifespan,
)


def _uptime() -> float:
    return round(time.time() - STATE["started_at"], 1)


@app.get("/health")
async def health() -> dict[str, Any]:
    uptime = _uptime()
    return {
        "status": "ok",
        "service": "genio-daemon",
        "version": VERSION,
        "phase": 1,
        "uptime_s": uptime,
        "agent": {
            "loop_running": STATE["loop_running"],
            "ticks": STATE["ticks"],
            "errors": STATE["errors"],
            "last_tick": STATE["last_tick"],
            "tick_seconds": TICK_SECONDS,
            "degraded": STATE["degraded"],
        },
    }


@app.get("/telemetry")
async def telemetry(fresh: bool = Query(False, description="bypass the TTL cache")) -> JSONResponse:
    snap = await asyncio.to_thread(system_monitor.snapshot, fresh)
    return JSONResponse(snap)


@app.get("/agent/status")
async def agent_status() -> dict[str, Any]:
    return {
        "version": VERSION,
        "ticks": STATE["ticks"],
        "errors": STATE["errors"],
        "loop_running": STATE["loop_running"],
        "last_tick": STATE["last_tick"],
        "last_tick_ms": STATE["last_tick_ms"],
        "degraded": STATE["degraded"],
        "tick_seconds": TICK_SECONDS,
        "ring": list(RING),
    }


def _shutdown(*_: Any) -> None:
    log.info("shutdown signal received")
    raise SystemExit(0)


def main() -> int:
    parser = argparse.ArgumentParser(description="Genio daemon (Phase 1)")
    parser.add_argument("--snapshot", action="store_true",
                        help="print one telemetry snapshot and exit")
    parser.add_argument("--host", default=HOST)
    parser.add_argument("--port", type=int, default=PORT)
    args = parser.parse_args()

    if args.snapshot:
        print(json.dumps(system_monitor.snapshot(force=True), indent=2, ensure_ascii=False))
        return 0

    import uvicorn  # imported late so --snapshot works without the server stack

    signal.signal(signal.SIGTERM, _shutdown)
    log.info("starting Genio daemon v%s on http://%s:%d", VERSION, args.host, args.port)
    uvicorn.run(app, host=args.host, port=args.port, log_level=LOG_LEVEL.lower())
    return 0


if __name__ == "__main__":
    sys.exit(main())

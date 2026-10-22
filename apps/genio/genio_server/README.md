# genio_server — core daemon (Phase 1)

FastAPI daemon with a background Agent Loop ("perceive" tick) and read-only
system telemetry.

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -r requirements.txt

python daemon.py --snapshot                  # one-shot telemetry dump
uvicorn daemon:app --host 127.0.0.1 --port 8765
curl -s localhost:8765/health | jq
curl -s localhost:8765/telemetry | jq '.cpu, .memory, .services'
curl -s localhost:8765/agent/status | jq '{ticks, last_tick, degraded}'
```

Environment: `GENIO_HOST`, `GENIO_PORT`, `GENIO_TICK_SECONDS`,
`GENIO_TELEMETRY_TTL`, `GENIO_RING_SIZE`, `GENIO_LOG_LEVEL`.

Phase 1 is read-only. Tool execution, capability tokens and approvals land in
Phase 2 (see `docs/audit/GENIO_MASTER_ARCHITECTURE.md`).

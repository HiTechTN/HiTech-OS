# Genio — Phase 3 Report: The Agentic Mind

**Status:** complete · **Commit:** `ea8f69a` · **Date:** 2026-10-22
**Scope:** give Genio a brain — a local-first model router, a bounded ReAct
loop, and persistent session memory — without weakening a single Phase 2
guarantee.

---

## 1. Executive summary

Phase 2 established that the model is never the authority. Phase 3 gives it
something to be authoritative *about*: a plan.

```
              ┌──────────────────────────────────────────────────────┐
   user ─────►│  PERCEIVE   telemetry + history + tool catalogue     │
              │     ↓                                                │
              │  PLAN       LLM proposes ONE JSON action (or final)  │
              │     ↓                                                │
              │  AUTHORIZE  broker: capability → risk → rate → audit │
              │     ↓                                ↓               │
              │  ACT        R0/R1 execute        R2 pause for human  │
              │     ↓                                ↓               │
              │  OBSERVE    result → memory → next prompt            │
              │     ↓                                                │
              │  REFLECT    final answer (or bounded stop)           │
              └──────────────────────────────────────────────────────┘
                          max_steps = 6 (hard bound)
```

Three new modules, zero new runtime dependencies:

| Component | File | Lines |
|---|---|---|
| Model router (Ollama → OpenRouter fallback, JSON repair) | `genio_server/core/model_router.py` | 347 |
| ReAct agent loop (bounded, approval-aware) | `genio_server/core/agent_loop.py` | 382 |
| SQLite session memory (durable, budgeted) | `genio_server/core/memory.py` | 228 |
| API + SSE integration | `genio_server/daemon.py` | 438 |
| CLI (trace, approval, SSE tail) | `genio_cli/genio.py` | 427 |
| Tests | `tests/test_model_router.py`, `test_agent_loop.py` | 33 new |

The model **cannot** call a tool. It can only emit
`{"action": {"tool": ..., "args": ...}}`; execution is still
`Broker.invoke()` with capability, risk, rate-limit, approval and audit
enforcement from Phase 2. The LLM is an untrusted planner, not a runtime.

---

## 2. Model router

### 2.1 Local first

| Backend | When | Default |
|---|---|---|
| `ollama` | always (unless `GENIO_MODEL=none`) | `qwen2.5:7b-instruct` @ `127.0.0.1:11434` |
| `openrouter` | only if `GENIO_OPENROUTER_KEY` / `OPENROUTER_API_KEY` is set | `meta-llama/llama-3.1-8b-instruct` |
| `scripted` | tests and offline demos | deterministic |

The machine thinks offline. The remote backend is a **fallback**, tried only
after the local call raises, times out or returns HTTP errors — never in
parallel, never by default. `qwen2.5:7b-instruct` was chosen because it is
already resident on this host (`gemma2` is not installed); `GENIO_MODEL`
overrides it.

Warm local latency measured during the live run: **~0.35 s** per short JSON
call (cold start ~9.7 s while Ollama loads weights).

### 2.2 JSON is a contract

`complete_json()`:

1. calls the model with Ollama's `format: "json"` (or OpenRouter
   `response_format={"type":"json_object"}`);
2. extracts the object with `parse_json_loose()` — plain JSON, ```json fences,
   prose around the object, trailing commas, or a single-element array;
3. if parsing fails, appends the bad answer plus a corrective instruction
   (*"Respond with ONE JSON object only — no prose, no fences"*) and retries;
4. after `GENIO_JSON_RETRIES` (default 2) it raises `JSONParseError`.

The parser is deliberately forgiving about *packaging* and strict about
*content*: it returns a `dict` or `None`, never a guess. 21 router tests cover
accepted shapes, rejected garbage, corrective retries, exhausted retries,
backend failover, and the Ollama request contract.

Per-backend stats (`ok`, `errors`, `last_error`, `last_ms`) are exposed through
`GET /mind/status` and attached to every agent result, so degradation is
observable rather than silent.

---

## 3. Agent loop

### 3.1 The plan contract

```json
{"thought": "one short sentence",
 "action": {"tool": "services.list", "args": {}},
 "final": null}
```

Rules enforced by the loop, not by the prompt:

* **one action per step** — a list, a bare string, or extra fields are ignored;
* **tool must exist** in `Broker.tools()` — otherwise the step is recorded as
  `denied/unknown_tool` and fed back as an observation (never executed);
* **args are untrusted** — the broker's validators (path traversal, unit
  allow-list, required/typed params) still run first;
* **`action: null` + `final`** ends the turn.

### 3.2 Bounds and fail-safes

| Control | Behaviour |
|---|---|
| `max_steps` (default 6, `GENIO_MAX_STEPS`) | hard bound on both loop iterations *and* model calls; exit status `max_steps` |
| Unknown tool | observation, retry within budget — no execution |
| `capabilitydenied` / `tokenexpired` / `tokeninvalid` / `audit_unavailable` / `replaydetected` | **immediate stop**, status `error` — policy failures are not retried |
| Model dies mid-run | stop planning, summarise the observations gathered, `degraded: true` |
| Model unavailable at step 1 | deterministic fallback planner (keyword → tool) and `degraded: true` |
| R2 action | persist pending intent → return `awaiting_approval` with `approval_id` |
| Resume with invalid/replayed token | status `error`, nothing executed, pending cleared |

The mid-run model-death rule came out of the test suite: the first
implementation re-planned with the fallback on every step and burned all six
steps repeating `telemetry.snapshot`. Now it stops and reports.

### 3.3 Approval pause and resume

```
POST /agent/message {"message": "restart cron.service"}
→ 200 {"status":"awaiting_approval",
       "approval":{"approval_id":"apr_…","tool":"service.restart",
                   "args":{"unit":"cron.service"},"expires_in_s":300},
       "session_id":"ses_…","steps":[…]}

POST /agent/approve {"session_id":"ses_…","approval_id":"apr_…"}
→ broker.approvals.grant() → single-use, args-bound token
→ loop.resume(). Broker re-verifies capability + nonce + args hash
→ 200 {"status":"completed"|"error", …}
```

The pending intent is stored in SQLite, so a pause survives a daemon restart.
A denial executes nothing and leaves the loop paused for a later decision.

---

## 4. Memory

`<GENIO_STATE_DIR>/memory/genio.db` (SQLite, WAL, directory mode 0700):

| Table | Contents |
|---|---|
| `sessions` | id, title, timestamps |
| `messages` | role (`user`/`assistant`/`tool`), content, metadata, ts |
| `tool_results` | tool, args, decision, ok, result, ts |
| `pending_actions` | session → tool, args, approval_id, step |

* **Working memory**: the last 16 messages / 12 000 characters are replayed to
  the model; truncation keeps the newest turns and drops the oldest.
* **Durability**: reopening the database restores transcript, observations and
  a paused approval (tested).
* **No secrets**: tokens and keys are never persisted — only conversation and
  intent. Tool results are capped at 8 000 characters.

Live proof of cross-turn memory (same session, no tool call):

```
you:  How is the machine doing right now?
genio: …CPU (13.7%) and memory (30.2%)… All services are running…
you:  What CPU percentage did you just report to me?
genio: The CPU percentage I reported is 13.7%.      [steps=0]
```

---

## 5. API and event stream

| Method | Path | Change |
|---|---|---|
| POST | `/agent/message` | now runs the ReAct loop (`session_id`, `approval_token` optional) |
| POST | `/agent/approve` | loopback-only human decision → resume |
| GET | `/mind/status` | router health, models, stats, max_steps, memory path |
| GET | `/memory/sessions` | recent sessions |
| GET | `/memory/{session_id}` | transcript + tool results + pending action |
| GET | `/agent/events` | SSE now carries the thinking lifecycle |

SSE phases emitted by the loop: `perceive`, `thought`, `plan`, `act`,
`observation`, `reflect`, `approval_required`, `final`, `error`, plus the
existing `tick` and `tool` streams. Captured live:

```
perceive   step=0 collecting system state
plan       step=1 step 1/6
plan       step=1 Using the services.list tool…
act        step=1 tool=services.list
observation step=1 tool=services.list
plan       step=2 step 2/6
plan       step=2 The number of running systemd services is 68.
reflect    step=2 goal satisfied
final      step=None 68
```

`genio.py --events` tails this stream in the terminal.

---

## 6. Verification

### 6.1 Automated — 75/75

| Suite | Tests | Focus |
|---|---|---|
| `test_model_router.py` | 21 | JSON extraction, repair retries, failover, Ollama contract |
| `test_agent_loop.py` | 12 | ReAct step/final, history reuse, step bound, approval pause/resume, denial stop, degradation, memory durability, budget |
| `test_security.py` | 28 | Phase 2 invariants (replayed approvals, traversal, frozen audit, unit allow-list) |
| `test_api.py` | 14 | health, tools, 401/403/409, approval over HTTP, memory endpoints, scripted agent loop |

```
75 passed in 0.66s
```

### 6.2 Live, with the real model (`qwen2.5:7b-instruct`)

| Scenario | Result |
|---|---|
| "How is the machine doing right now?" | 2 planning calls, `telemetry.snapshot` executed, natural-language answer, **2.9 s** end-to-end |
| "How many systemd services are running?" | `services.list` → `68`, **~2 s** |
| "What CPU percentage did you just report?" | answered `13.7%` from session memory, **0 steps** |
| "Restart cron.service" | `approval_required` → Y → grant → `execution_failed` (polkit denies an unprivileged daemon) |
| `/agent/events` during a run | full perceive→plan→act→observe→reflect trace |

The restart failure is the honest outcome on this host: approval authorises
the attempt, it does not grant OS privilege. On the NixOS target the daemon
runs as root and the same path returns `executed`
(`test_r2_with_human_approval_executes` covers it).

---

## 7. Known limitations / Phase 4 handoff

1. **Prompt injection is contained, not solved.** Extracted documents are
   sanitised to text, and the model can only propose broker-validated calls —
   but a malicious document can still steer *which* R0 tool is called. Phase 4:
   provenance labels in the prompt + per-step intent logging.
2. **SSE broadcasts tool arguments** to any local subscriber (including
   `file.write` content). Phase 4: redact/truncate args in events.
3. **Single model, no per-task routing.** `model_router` supports a chain, not
   a policy (`small model for classification, large for planning`).
4. **No token streaming.** Calls are request/response; the UI shows phase
   transitions, not partial text.
5. **Context is transcript-only.** No embeddings or long-term facts yet;
   `messages` is the whole memory.
6. **Approval TTL is 300 s**, pending intents survive restart but expire in the
   store — a restart after expiry requires a fresh request.
7. **Dev signing key** unchanged from Phase 2 (`key_source: dev`); production
   provisions `GENIO_CAP_KEY` via NixOS.

---

## 8. Reproduce

```bash
cd apps/genio
/tmp/genio-venv/bin/python -m pytest tests -q

mkdir -p /tmp/genio-state
cd genio_server
GENIO_STATE_DIR=/tmp/genio-state GENIO_MODEL=qwen2.5:7b-instruct \
  /tmp/genio-venv/bin/python daemon.py --port 8770 &
cd ..
/tmp/genio-venv/bin/python genio_cli/genio.py --url http://127.0.0.1:8770 \
  --once "How is the machine doing right now?"
/tmp/genio-venv/bin/python genio_cli/genio.py --url http://127.0.0.1:8770 --events
```

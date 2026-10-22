# Genio — Phase 2 Report: Capability Broker, Audit Chain & CLI

**Status:** complete · **Commit:** `4b6a4da` · **Date:** 2026-10-22
**Scope:** turn the Phase 1 read-only daemon into an agent that can *act* on the
system without ever trusting the model.

---

## 1. Executive summary

Phase 2 answers the only question that matters before an AI may touch a
machine: **who authorises the action?**

The model is never the authority. A tool call is only a *request*; execution
requires three independent things to line up:

1. a **capability token** (signed, expiring, scoped to one capability),
2. a **risk decision** made by the broker (R0/R1/R2), and for R2
3. a **human approval token** that is single-use, bound to the exact tool *and*
   the exact arguments.

Every decision — authorised, denied, failed — lands in an append-only
**SHA-256 hash chain**. If the chain cannot be written, privileged actions are
**frozen** (fail-closed), while observation (R0) keeps working.

| Component | File | Lines |
|---|---|---|
| Capability tokens | `genio_server/core/caps.py` | 207 |
| Broker, audit chain, approvals, tools | `genio_server/core/broker.py` | 607 |
| HTTP API + SSE | `genio_server/daemon.py` | 401 |
| CLI/TUI client | `genio_cli/genio.py` | 314 |
| Adversarial tests | `tests/test_security.py`, `tests/test_api.py` | 325 · 36 tests |

---

## 2. Threat model

| Threat | Mitigation | Proof |
|---|---|---|
| Model hallucinates a destructive command | Model output is a *suggestion*; only `Broker.invoke` executes, and only allow-listed tools with validated args | `test_invoke_without_token_is_401` |
| Prompt injection inside an extracted document | Extraction is R0 but returns **sanitised plain text** (no HTML/script), truncated to 8 000 chars, and never auto-executes | `_sanitize_html`, `store.extract` |
| Privilege escalation via forged token | HMAC-SHA256 over canonical JSON; signature compared with `hmac.compare_digest` | `test_tampered_signature_is_refused` |
| Replay of an approved action | Single-use nonce + 120 s TTL; nonce cache purged on expiry | `test_replayed_approval_nonce_is_refused` |
| Approval reused for a different target | Approval token carries `args_hash`; mismatch → `approval_mismatch`, handler never runs | `test_approval_is_bound_to_arguments` |
| Path traversal / arbitrary write | `_resolve_under()` resolves and requires `relative_to(root)`; writes confined to `<state>/files` | 6 traversal vectors blocked |
| `systemctl` argument injection | Regex allow-list + deny-list (ssh, dbus, getty, user@, systemd-\*, genio) and `shell=False` argv execution | 9 malicious unit names refused |
| Audit tampering | Each record hashes `prev_hash + body`; verification recomputes the whole chain | `test_audit_chain_valid_then_tamper_detected` |
| Disk full / read-only FS hiding actions | `ensure_writable()` runs **before** any R1/R2 side effect → `audit_unavailable` | `test_unwritable_audit_freezes_r1_r2_but_not_r0` |
| R2 executed by a script without a human | No approval token ⇒ `approval_required` (HTTP 409), handler never called | `test_r2_without_approval_is_refused` |
| Runaway automation | Per-tool, per-subject rate limits (R2: 3/min) | `test_r2_rate_limit` |

**Explicit non-goals in Phase 2:** the dev signing key (`GENIO_CAP_KEY` unset →
`key_source: dev`) and the loopback session bootstrap are *development*
mechanisms. Phase 3 replaces both with the hitech-store admin OTP flow and a
machine-bound key.

---

## 3. Capability tokens

Format: `g1.<base64url(payload)>.<base64url(HMAC-SHA256)>`, canonical JSON
(sorted keys, compact separators).

```json
{"v":1,"kind":"session","sub":"cli","caps":["fs:write","service:read"],
 "iat":1761164400,"exp":1761207600,"nonce":"…","reuse":true,"key":"dev"}
```

* `kind=session` → reusable, minted at `POST /caps/session` (loopback only).
* `kind=approval` → **forced single-use**, requires `tool` + `args_hash`, and is
  only minted by `ApprovalStore.grant()` after a human decision.
* Verification checks: structure → signature → version → expiry (with 2 s
  skew) → required capability → nonce. Failures map to stable codes:
  `tokeninvalid`, `tokenexpired`, `capabilitydenied`, `replaydetected`,
  `approval_invalid`, `approval_mismatch`, `approval_replayed`.

Capabilities in use: `telemetry:read`, `service:read`, `store:read`, `fs:write`,
`service:restart`, plus derived `approval:<tool_id>`.

---

## 4. Risk classes and the tool registry

| Risk | Meaning | Gate | Tools |
|---|---|---|---|
| **R0** | Observe | capability only, best-effort audit | `telemetry.snapshot`, `services.list`, `service.status`, `store.list`, `store.extract` |
| **R1** | Reversible write in Genio's own sandbox | capability + mandatory pre/post audit + rate limit | `file.write` (root: `<state>/files`) |
| **R2** | Privileged, affects the host | capability + **human approval token** + audit + rate limit | `service.restart` |

Semantic validators run **before** the risk gate, so a human is never asked to
approve an action whose arguments are already illegal (this was caught by the
first test run: traversal and bad unit names were reaching the approval prompt).

---

## 5. Approval flow (R2)

```
CLI                       Daemon (broker)                   Human
 │  invoke(unit)            │                                 │
 ├─────────────────────────►│ validate → rate → audit writable│
 │◄── 409 approval_required │ ApprovalStore.request()         │
 │    + approval_id         │ audit: approval_requested       │
 │                                                           │
 │  "Approve restart cron.service? [y/N]" ──────────────────►│
 │◄────────────────────────────────────────────── yes ───────┤
 │  POST /approvals/{id}/grant                               │
 ├─────────────────────────►│ caps.issue(kind=approval,        │
 │◄── approval_token ───────┤   tool, args_hash, ttl=120)     │
 │  invoke(unit, token)     │                                 │
 ├─────────────────────────►│ verify → nonce burn → args_hash │
 │                          │ audit: authorized               │
 │                          │ execute → audit: executed       │
 │◄── result ───────────────┤                                 │
```

A denied or timed-out prompt executes nothing. A second use of the same token
returns `approval_replayed` and the handler is not called again.

---

## 6. Audit chain

`<state>/audit/genio-audit.jsonl`, one JSON record per line:

```json
{"seq":7,"ts":"2026-10-22T21:41:42+00:00","actor":"cli","tool":"service.restart",
 "risk":"R2","args_hash":"…","decision":"executed","detail":"cron.service",
 "prev":"caf5…","hash":"7902…"}
```

`hash = SHA-256(prev_hash + canonical(body))`, genesis `prev = "0"*64`.
`GET /audit/verify` recomputes the chain and reports
`{"ok":true,"records":N,"head":…}`; a single altered byte yields
`{"ok":false,"error":"hash mismatch at seq 1"}`.

Audit records are written **before** and **after** every R1/R2 execution; if
the pre-write fails the action is refused with `audit_unavailable`.

---

## 7. HTTP API

| Method | Path | Purpose | Codes |
|---|---|---|---|
| GET | `/health` | liveness, loop, audit/broker state | 200 |
| GET | `/telemetry`, `/agent/status` | read-only snapshots | 200 |
| POST | `/caps/session` | dev bootstrap token (loopback only) | 200, 403 |
| GET | `/tools` | catalogue with risk/cap/rate | 200 |
| POST | `/tools/{id}/invoke` | the only execution path | 200, 401, 403, 409, 500 |
| GET | `/approvals` | pending R2 requests | 200 |
| POST | `/approvals/{id}/grant` | human decision → approval token | 200, 404 |
| GET | `/audit/verify`, `/audit/tail?n=` | chain integrity / recent records | 200 |
| POST | `/agent/message` | deterministic router (no execution) | 200 |
| GET | `/agent/events` | SSE: `tick`, `tool`, `message`, `approval` | stream |

Status mapping is deliberate: **403 = denied** (bad token, missing cap, fused
audit), **409 = approval_required**, **500 = execution_failed**.

---

## 8. CLI

```bash
python genio_cli/genio.py                     # interactive TUI
python genio_cli/genio.py --once "status"     # one message
python genio_cli/genio.py --demo --yes        # scripted proof
```

The CLI holds no privileges and no keys: it obtains a session token, renders
telemetry with `rich`, and asks the operator **Y/N** whenever the daemon answers
409. Commands: `status · tools · audit · approvals · call <tool> <json> · ask`.

---

## 9. Verification

### 9.1 Automated — 36/36 passing

```
tests/test_security.py  27 passed
tests/test_api.py        9 passed
36 passed in 0.48s
```

Coverage highlights: R2 refused without approval; approval executes; nonce
replay refused; args-bound mismatch refused; forged token refused; expired token
refused; missing capability refused; 6 traversal vectors blocked; writes stay
inside root; unwritable audit freezes R1/R2 while R0 continues; chain tamper
detected; 9 dangerous unit names refused; valid unit escalates to approval; R2
rate limit enforced; 401 without `X-Genio-Cap`; 409 + `approval_id`; 404 on
unknown approval; 403 when audit is frozen.

### 9.2 Live end-to-end (`--demo`, real daemon, real /proc)

```
connected · Genio v0.2.0-phase2 · tools 7 · audit writable
1 · telemetry (R0)     cpu 5.2% of 12 cores · mem 29.5% · 68 services · 0 failed
2 · tool catalogue     7 tools, R0/R1/R2 tagged
3 · services.list      executed (audit #1)
4 · file.write         executed (audit #3) → <state>/files/demo/hello.txt
5 · service.restart    REFUSED → approval_required (audit #4)
6 · human approval     granted → authorised (audit #6) → failed (audit #7)
7 · audit chain        ok: true, 7 records, head 7902a7e8…
```

Step 6 is intentionally reported as **failed**, not executed: the CLI is
unprivileged, so `systemctl restart` was refused by polkit. *Approval authorises
the attempt; it does not grant OS privileges.* On the NixOS target the daemon
runs as root and the same path returns `executed` (covered by
`test_r2_with_human_approval_executes`).

SSE verified live:

```
event: hello   data: {"version":"0.2.0-phase2"}
event: tick    data: {"cpu_percent":5.3,"mem_percent":29.6,"services_running":68}
```

---

## 10. Known limitations / Phase 3 handoff

1. **Dev signing key** when `GENIO_CAP_KEY` is absent — startup logs
   `key_source: dev`; production must provision a key file (0600) via NixOS.
2. **Session bootstrap** is loopback-only and unauthenticated; Phase 3 replaces
   it with hitech-store admin OTP + machine-bound claims.
3. **Router is deterministic**, not an LLM. Phase 3 adds the planner, which may
   only *propose* tool calls through the same broker.
4. **Single-host audit** — no remote anchoring yet (Phase 5: Merkle root to the
   store drive / external witness).
5. **`systemctl` waits on polkit** for unprivileged daemons (bounded to 8 s and
   reported as failure). Root service unit removes this.
6. **No streaming tool output** — invoke is request/response; Phase 3 adds
   progress events on `/agent/events`.

---

## 11. Reproduce

```bash
python3 -m venv /tmp/genio-venv
/tmp/genio-venv/bin/pip install -r genio_server/requirements.txt -r requirements-dev.txt

cd apps/genio
/tmp/genio-venv/bin/python -m pytest tests -q

mkdir -p /tmp/genio-state
cd genio_server
GENIO_STATE_DIR=/tmp/genio-state /tmp/genio-venv/bin/python daemon.py --port 8770 &
cd ..
/tmp/genio-venv/bin/python genio_cli/genio.py --url http://127.0.0.1:8770 --demo --yes
curl -s http://127.0.0.1:8770/audit/verify | python3 -m json.tool
```

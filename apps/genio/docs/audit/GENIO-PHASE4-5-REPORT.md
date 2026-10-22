# Genio — Phase 4 & 5 Report: Capability System + Policy Engine

**Status:** complete · **Commit:** `dba76be` · **Date:** 2026-10-22
**Scope:** turn Genio from a system agent into a *universal portal* agent —
documents, media, directories — while making authorization explicit,
deterministic and auditable **before** any tool ever runs.

---

## 1. Executive summary

Phase 2 proved that the model is never the authority. Phase 3 gave the model
a bounded plan. **Phase 4 & 5 remove the last implicit trust**: the model's
choice is now evaluated by a policy engine whose rules are data, not prompt
text, and whose deny decisions are written to the same hash-chained audit log
as executions.

```
user message
   │
   ▼
AgentLoop ── verify session token ──────────────► caps.verify(consume=False)
   │                                               (caps set, subject)
   ▼
PLAN (untrusted LLM JSON)
   │
   ▼
PolicyEngine.evaluate(spec, subject, caps, args)
   ├── DENY ────────────► audited · loop stops · no broker call
   ├── REQUIRE_CONFIRM ► broker R2 approval (signed, single-use, args-bound)
   └── ALLOW ───────────► Broker.invoke()
                           ├── capability re-check
                           ├── argument validation (pre-risk)
                           ├── rate limit
                           ├── policy re-check (defence in depth)
                           ├── audit writable (fail-closed)
                           └── execute / deny
```

Nine tools are published through a duplicate-safe registry. Four of them
open the hitech-store portal: list directories, extract PDF/DOCX/XLSX/PPTX/
HTML/CSV/text, inspect audio/video/image metadata, and create folders.

**No new runtime dependency.** Still Python stdlib + FastAPI/uvicorn/psutil.

| Component | File | Lines |
|---|---|---|
| Tool registry + hitech-store tools + validators | `genio_server/core/tool_registry.py` | 306 |
| Deterministic policy engine (9 rules) | `genio_server/core/policy_engine.py` | 200 |
| hitech-store HTTP client (auth, Range, errors) | `genio_server/core/store_client.py` | 207 |
| Broker (refactored: tags, system/store split, policy gate) | `genio_server/core/broker.py` | 600 |
| Agent loop (session verification + policy phase) | `genio_server/core/agent_loop.py` | 449 |
| API (`/policy`, `/policy/evaluate`, `/portal/status`) | `genio_server/daemon.py` | 489 |
| Tests | `tests/test_policy_engine.py`, `test_tool_registry.py`, updates | 81 new |

**Test suite: 156 passing** (Phase 3 baseline was 75).

---

## 2. Tool registry (Phase 4)

`ToolRegistry` is the single source of truth for *what Genio can do*. A
`ToolSpec` carries `id`, risk, description, capability, JSON-ish param schema,
rate limit, summary function, optional **pre-risk validator**, handler, and
`tags`. The registry rejects invalid or duplicate ids and exposes
`by_tag()`, `by_capability()`, `catalogue()` and `summary()` for the API and
the planner prompt.

### 2.1 Published tools

| Tool | Risk | Capability | What it does |
|---|---|---|---|
| `telemetry.snapshot` | R0 | `telemetry:read` | CPU/memory/load/disk snapshot |
| `services.list` | R0 | `service:read` | Running/failed units |
| `service.status` | R0 | `service:read` | `systemctl show` for one unit |
| `store.list` | R0 | `store:read` | Directory listing (folders, files, sizes, checksums) |
| `store.extract` | R0 | `store:extract` | Text from PDF, DOC/DOCX, XLS/XLSX, PPT/PPTX, HTML, CSV/TSV, text |
| `store.media_info` | R0 | `media:read` | Audio/video/image kind, size, SHA-256, preview URL — **metadata only** |
| `file.write` | R1 | `fs:write` | Sandboxed write under `<state>/files` |
| `service.restart` | R2 | `service:restart` | Restart a non-critical unit (human approval) |
| `store.mkdir` | R2 | `store:write` | Create folder under `public/` or `agents/` (human approval) |

`store.list`, `store.extract` and `store.media_info` are classified under
`by_tag("store")`; extract is additionally `documents`, media is `media`,
mkdir is `directories`.

### 2.2 Validators run *before* the risk gate

A human must never be asked to approve an action whose arguments are already
illegal. Every store tool validates first:

- `check_store_path` — normalises a leading `/` (`/public/x.pdf` →
  `public/x.pdf`), rejects absolute escapes, `..`, empty segments, NUL,
  paths over 400 chars, and roots outside `public|private|agents`.
- `check_new_name` — `fullmatch` on `[A-Za-z0-9._ -]{1,64}`, rejects `.` and
  `..` (the `fullmatch` matters: `$` alone accepts a trailing newline).
- `store.extract` rejects unsupported extensions (`exe`, `bin`, …) with a
  clean `invalid_args`, not an execution failure.

### 2.3 Text safety

Extracted HTML is untrusted input. `sanitize_html()` removes
`<script>`/`<style>` blocks **before** stripping tags, collapses whitespace
and caps extraction at 8 000 chars / 2 MiB download. The model receives text,
never raw markup, and media bytes are never fetched — only metadata and
preview URLs.

---

## 3. Policy engine (Phase 5)

`PolicyEngine.evaluate(spec, subject, caps, args) -> PolicyDecision`
returns one of three decisions, a machine code, a human reason, and the rule
that fired. It is pure and deterministic: the same inputs always produce the
same decision, and arguments cannot influence the engine except through the
path/write-scope checks.

### 3.1 Rules

| Rule | Decision | Code | Trigger |
|---|---|---|---|
| P1 | DENY | `unknown_tool` | tool id not in the registry |
| P2 | DENY | `denied_by_policy` | hard deny-list (cannot be unlocked by any capability) |
| P3 | DENY | `subject_blocked` | subject explicitly blocked |
| P4 | DENY | `capabilitydenied` | session lacks the tool capability (no wildcard) |
| P5 | DENY | `private_store_denied` | `private/…` without `store:private` |
| P6 | DENY | `write_scope_denied` | `store.mkdir` outside `public/` or `agents/` |
| P7 | REQUIRE_CONFIRMATION | `requires_human_approval` | any R2 tool |
| P8 | ALLOW | `allowed_r1` | R1 with capability |
| P9 | ALLOW | `allowed` | R0 with capability |

Hard deny-list: `store.delete`, `store.purge`, `store.rmdir`, `shell.exec`,
`system.reboot` — these tools do not exist yet, but the policy already
refuses them if a future registry registers them by mistake.

`matrix(tools, subject, caps)` renders every tool × decision for the UI;
`rules_snapshot()` exposes rules, deny-list and `store:private` for audit.

### 3.2 Enforcement order

1. **Session token** verified by the agent loop before the first model call
   (`consume=False`: the token is reusable for the session, nonces still
   prevent replay of approvals).
2. **Policy** runs inside the loop, before the broker; a DENY stops the loop,
   records `denied` in the audit chain and emits a `policy` event. No model
   replanning after a policy stop.
3. **Broker** repeats the policy check (defence in depth for direct API
   callers and future callers that bypass the loop).
4. **Risk gate** — R2 still needs a signed, single-use, argument-bound
   human approval token (Phase 2 mechanics unchanged).
5. **Audit writability** is checked before any R1/R2 side effect (fail-closed).

The pending-approval payload now includes the full policy verdict
(`decision`, `code`, `rule`, `capability`), so a human sees *why* the system
is pausing.

---

## 4. hitech-store client

`StoreClient.from_env()` reads `HITECH_STORE_URL`
(default `http://127.0.0.1:8095`), `HITECH_STORE_ADMIN_TOKEN` and
`HITECH_STORE_AGENT_TOKEN`.

| Concern | Behaviour |
|---|---|
| auth mode | `admin` (cookie `hitech_admin_token`) → `agent` (`X-Agent-Token`) → `anonymous` |
| listing | `/api/drive/list?path=…` |
| documents | `/api/drive/extract?path=…` (server-side PDF/Word/Excel/PowerPoint renderer) |
| text/CSV/HTML | `/api/drive/download` with `Range: bytes=0-2097151` (2 MiB cap) |
| media | listing metadata + preview/download URLs; **no byte download** |
| mkdir | `POST /api/drive/mkdir`, requires admin cookie |
| errors | 401/403 → `StoreAuthError`, 404 → `StoreNotFound`, others → `StoreError`; HTTP 403 on mkdir becomes a human-readable `RuntimeError` → `execution_failed` |

`STORE_ROOTS = ("public", "private", "agents")` mirrors the store's layout.
`describe()` reports base URL, auth mode and whether credentials are present
without ever printing a token.

---

## 5. API additions

| Route | Purpose |
|---|---|
| `GET /health` | now exposes `registry` (count, risk mix, capabilities), `policy` (rules, deny-list) and `portal` (auth mode, no secrets) |
| `GET /policy` | rules + decision matrix for the default session capabilities |
| `POST /policy/evaluate` | stateless what-if: `{tool, caps?, args?}` → verdict; never executes |
| `GET /portal/status` | store health with honest `ok: false` on failure |

`DEFAULT_SESSION_CAPS` now includes `store:extract`, `store:write`,
`media:read` in addition to `store:read`; `store:private` is deliberately
**not** granted by default. VERSION is `0.4.0-phase4-5`, `/health.phase = 5`.

---

## 6. Verification

### 6.1 Tests — 156 passing

| File | Tests |
|---|---|
| `test_security.py` | 28 |
| `test_api.py` | 19 |
| `test_model_router.py` | 21 |
| `test_agent_loop.py` | 16 |
| `test_policy_engine.py` | 25 |
| `test_tool_registry.py` | 47 |

Policy tests cover every rule, wildcard capability, determinism (100
repetitions), matrix completeness, and broker enforcement (a deny-listed tool
never invokes its handler). Registry tests cover registration errors, tags/
capability queries, path and folder-name validation, all format dispatch
branches (PDF/HTML/CSV/text), auth errors, `Range` downloads and HTTP error
mapping. Agent tests cover private-path denial (no execution, no replanning),
public-path allow, invalid session tokens, and the policy verdict inside the
approval payload.

### 6.2 Live run against the real store (`:8095`, read-only)

Daemon: `0.4.0-phase4-5`, 9 tools, 8 capabilities, 9 policy rules.

| Operation | Result |
|---|---|
| `store.list public` | 1 directory (`HCF`), 11 files (DOCX, PDF, MP4/WEBM, ZIP) |
| `store.extract` PDF | `format=pdf`, 2 015 chars ("UniEmens Integrazione software di controllo", Giugno 2025) |
| `store.extract` DOCX | `format=word`, 322 chars (checker Uniemens procedure) |
| `store.media_info` MP4 | `kind=video`, 13.80 Mo, SHA-256 `50db9444…`, preview URL, "metadata only" |
| `store.extract private/secret.pdf` | `denied` / `private_store_denied` — P5, no store call |
| `store.extract public/../etc/passwd` | `denied` / `invalid_args` (HTTP 403), handler never runs |
| `store.mkdir public/genio-demo` | `approval_required` → human grant → `execution_failed`: "admin credentials are not configured for Genio" (403 *Session admin requise*), honest and audited |
| audit chain | 21 records, `verify_chain` OK, head `151f5f5e…` |

Agent end-to-end (Ollama `qwen2.5:7b-instruct`):

- "Quanti file e quante cartelle ci sono nella cartella public?" → 2 steps
  (the model first guessed a filesystem-style path, the broker rejected it
  cleanly, the model corrected), answer **"1 folder and 11 files"**.
- "Riassumi in due frasi public/LANCIO UNIEMENS.docx" → 1 step, correct
  322-char extraction summarised.
- "Leggi public/UniEmens_Controllo_2025-06_PDF.pdf e riassumilo" → extract
  executed; policy ALLOW each step.
- CLI `--once "Quanti servizi stanno girando adesso?"` → 68 services, trace
  shows one `services.list` call.

### 6.3 Bugs found and fixed during verification

1. **Two `ToolArgError` classes** — `broker` and `tool_registry` each defined
   their own; a traversal path escaped the broker's `except` and produced
   HTTP 500. The registry now imports the broker's class (single source of
   truth) and a regression test asserts `invalid_args`.
2. **Absolute-looking paths** — `/public` was rejected outright; the validator
   now strips a leading slash and normalises (`/public/x.pdf` →
   `public/x.pdf`), while unknown roots and `..` remain fatal.
3. **`$` on folder names** — `$` in Python accepts a trailing newline;
   `check_new_name` now uses `fullmatch`.
4. **Repeated identical tool calls** — the local model re-extracted the same
   PDF three times. The planner prompt now forbids repeating a call whose
   observation is already in context (verified: 1 step afterwards).
5. **Policy matrix lied about R2** — evaluating with empty args showed
   `write_scope_denied`; the matrix is now a risk-only what-if (`args=None`)
   that displays `REQUIRE_CONFIRMATION`, while real invocations still check
   paths.

---

## 7. Honest limitations

- **No admin token in the deployment**: `store.mkdir` cannot actually write
  yet. Genio reports `execution_failed` with the store's message; it never
  pretends success. Provision `HITECH_STORE_ADMIN_TOKEN` (a short-lived admin
  cookie JWT) to enable writes.
- **XLSX/PPTX/HTML/CSV** are unit-tested but no live sample exists in
  `public/`; server-side extract currently supports PDF/Word/Excel/PowerPoint,
  while HTML/CSV/text use the download path.
- **Media is metadata-only** by design; Genio does not fetch or transcribe
  audio/video bytes.
- **Local model quality**: `qwen2.5:7b-instruct` occasionally guesses a
  filesystem-style path; the loop recovers on the next turn and the policy
  never executes an invalid path. Prompt hardening reduced, but cannot
  eliminate, this.
- The `:8095` store is the user's own dev instance; all verification was
  read-only except the approved (and refused) `mkdir` attempt.

---

## 8. Next

- Phase 6: portal UX — surface `/policy` matrix and pending approvals in the
  Web GUI; document preview via the existing store URLs.
- Provision the admin token and verify a real `store.mkdir` end-to-end.
- Optional: Excel/PowerPoint live fixtures, media transcription as an
  explicitly-capped capability, and a `store.move` tool (R2) with the same
  write-scope policy.

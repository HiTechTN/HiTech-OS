# Genio — Phase 6 Report: Real-Time Web GUI (Chronos Portal)

**Date:** 2026-10-22  
**Scope:** `apps/genio/genio_web` + Phase 6 daemon endpoints  
**Status:** Complete, verified in a live browser against the real daemon  
**Commit:** `feat(genio): implement real-time React web GUI for daemon interaction and R2 approvals`

---

## 1. Executive summary

Phase 6 replaces the placeholder/simulated web surface with a real React
application wired to the Genio daemon. The **Chronos Portal** is a single-page
operator console with five views:

- **Console** — chat, live agent trace, R2 approval queue and host vitals.
- **Telemetry** — CPU/memory history, disks, sensors, services, processes.
- **Policy** — the nine ordered rules and the per-tool decision matrix.
- **Tools** — the nine-tool registry with capabilities, arguments and tags.
- **Audit** — SHA-256 chain verification and recent decisions.

The daemon now exposes the built SPA at `/ui/`, redirects `/` to it, and adds
`POST /approvals/{id}/deny` so an operator can cancel a paused R2 turn instead
of only granting it. Every action still goes through the same capability,
policy, risk-gate, approval-token and audit path as the CLI; the GUI is a
presentation and decision surface, not a bypass.

Verification was end-to-end against the live daemon and hitech-store:
list/read operations, a chat round-trip, a granted `store.mkdir` (honest
`execution_failed` because no store admin token is configured), a denied
`store.mkdir` (turn cancelled, nothing executed), all five views, and the audit
chain (`valid`, 30 records). Test suite: **162 passing**.

---

## 2. Goals and non-goals

### Goals

1. Replace the simulated web page with a real client of the daemon API.
2. Stream the agent's phases live (SSE) and keep a usable trace across
   reconnects.
3. Make R2 approvals a first-class, safe workflow: exact arguments, policy
   verdict, expiry, single-use grant, explicit denial.
4. Surface telemetry, policy, tools and audit without duplicating daemon logic.
5. Keep the deployment self-contained: one origin, no CDN, no runtime config
   service, no client-side secrets.

### Non-goals

- Multi-user accounts, roles or remote authentication.
- Persisting conversations across reloads.
- Editing policy rules from the browser.
- Replacing the CLI or the daemon as the source of truth.

---

## 3. Architecture

```text
┌──────────────────────────── browser (same origin) ────────────────────────────┐
│  React 19 + TypeScript SPA (built assets at /ui/)                             │
│                                                                                │
│  GenioProvider                                                                 │
│    ├─ polling: /health /telemetry /agent/status /approvals /policy /tools      │
│    ├─ SSE:     /agent/events  (agent, tick, tool, approval, hello)             │
│    ├─ chat:    POST /agent/message  (new turn or approval resume)              │
│    └─ decide:  POST /approvals/{id}/grant | /deny                              │
└───────────────────────────────────┬────────────────────────────────────────────┘
                                    │ loopback HTTP (127.0.0.1:8770)
┌───────────────────────────────────▼────────────────────────────────────────────┐
│ FastAPI daemon                                                                 │
│   /caps/session → signed, capability-scoped token                              │
│   AgentLoop: session verify → policy → broker → risk gate → audit              │
│   Broker: capability + policy re-check + rate limit + approval token           │
│   SSE broadcaster: ticks + agent/tool/approval events                          │
│   StaticFiles: genio_web/dist mounted at /ui (html=True), / → 307 /ui/         │
└───────────────────────────────────┬────────────────────────────────────────────┘
                                    │
                    telemetry / systemd / hitech-store (read-only default)
```

### Data flow for an agent turn

1. The client bootstraps a session with `POST /caps/session` (`subject=web-gui`)
   and stores the token in `localStorage` (`genio.session.v1`).
2. `POST /agent/message` starts a turn. The loop verifies the token, emits
   `perceive`, `thought`, `plan`, then evaluates **policy before execution**.
3. The broker validates arguments, re-checks the policy verdict, applies the
   risk gate and rate limit, then executes or pauses.
4. R2 calls return `approval_required` with an approval id; the UI opens a modal
   containing the exact arguments and the policy rule/verdict.
5. **Grant** calls `/approvals/{id}/grant` (single-use, argument-bound, 120 s),
   then resumes the same session with `approval_token`. **Deny** calls
   `/approvals/{id}/deny`, which removes the request and cancels the paused turn.
6. The final answer, steps and audit records are rendered; the chain can be
   verified from the Audit view.

---

## 4. Daemon changes

| Change | Detail |
| --- | --- |
| `POST /approvals/{id}/deny` | Body `{approver, session_id?, reason?}`; removes the pending request via `ApprovalStore.deny()` and, when a session is supplied, cancels the paused action via `AgentLoop.cancel_pending()`. Emits an `approval` SSE event with `decision="denied"`; unknown/expired ids return 404. |
| `AgentLoop.cancel_pending()` | Clears the paused state without executing or resuming, appends a cancellation message and emits a `reflect` event (`approval denied: …`). |
| `ApprovalStore.list()` | Now excludes granted requests, so the UI queue reflects only decisions still waiting for a human. |
| `GET /tools` | Each tool now includes `tags` in addition to `cap`, `risk`, `params`, `rate_per_min` and `requires_approval`; the UI uses tags for filtering. |
| Static UI | If `genio_web/dist` exists at import time, mount it at `/ui/` and redirect `/` → `/ui/`; otherwise the API-only daemon is unchanged. |
| Version | `0.6.0-phase6`, `/health.phase = 6`. |

The deny endpoint is deliberately **not** an execution path: it only discards
the request and clears the paused turn. There is no token to replay.

---

## 5. Frontend implementation

```text
genio_web/
  index.html
  vite.config.ts        dev proxy + "/ui/" production base
  tsconfig.json
  src/
    main.tsx
    App.tsx
    index.css           Tailwind v4 theme (dark sovereign palette)
    lib/api.ts          typed fetch client; accepts agent error bodies as JSON
    lib/types.ts        daemon payload types
    lib/util.ts         time/size formatting, policy copy, risk classes
    state/GenioProvider.tsx
    components/
      icons.tsx  ui.tsx  Shell.tsx
      Console.tsx  ApprovalModal.tsx
      TelemetryView.tsx  PolicyView.tsx  ToolsView.tsx  AuditView.tsx
```

### State and streaming

`GenioProvider` is the single source of client state: health, telemetry,
history, tools, policy, session, chat messages, SSE events and the approval
queue. It:

- polls health/status/approvals every few seconds and telemetry/audit per view;
- subscribes to `/agent/events` with `EventSource`, reconnects automatically and
  marks the connection `SSE LIVE` / offline;
- appends each agent phase to the active trace and routes `approval` events into
  the modal + queue;
- de-duplicates approvals by id and supports focusing a request from the queue;
- reconstructs the trace from the POST response `steps` when SSE reconnects
  mid-turn, so the UI never shows an empty trace after a network blip.

### Grant / deny semantics

The modal shows the tool, risk class, exact JSON arguments, policy rule
(`P7_r2_confirmation`) and verdict, a countdown, and the approval id. Grant
issues the token and resumes the session; deny removes the request and patches
the assistant message to “Action denied by the operator. Nothing was executed.”
Both paths clear the queue and refresh `/approvals`.

### Agent error handling

Agent failures (`execution_failed`, `capabilitydenied`, etc.) are valid JSON
responses with HTTP 500. The client treats a JSON body with `status` as an
`AgentResponse` for `/agent/message` (new turn and resume), so the operator sees
the honest error in the conversation instead of a generic transport error.
Other endpoints keep strict non-2xx handling (`ApiError`).

---

## 6. Security posture

- **Same origin only.** The daemon serves the SPA; the browser never sends
  credentials cross-origin. No API key is embedded in the bundle.
- **Capabilities stay server-side.** The session token is signed and scoped; the
  UI cannot mint or widen capabilities. Defaults remain 8 capabilities and
  exclude `store:private`.
- **No direct tool execution.** Every proposal goes through policy and the
  broker. The UI cannot call a handler or forge a policy verdict.
- **Approval is explicit and narrow.** A grant token is single-use,
  argument-bound and expires after 120 s. Denial is irreversible for that turn.
- **No raw HTML rendering.** Model/store text is rendered as text by React;
  there is no `dangerouslySetInnerHTML`.
- **Loopback trust is the boundary.** The daemon binds `127.0.0.1`; any local
  process can call the API, including grant/deny. The portal must sit behind an
  authenticating reverse proxy before being exposed beyond the host.
- **Fail-closed.** Missing/invalid session tokens, policy denials, unwritable
  audit state and store write failures all stop execution and are surfaced,
  not hidden.

---

## 7. Verification evidence

Environment: Pop!_OS 22.04, Node 22.23.3 / npm 10.9.9, daemon on
`127.0.0.1:8770`, state `/tmp/opencode/genio-state`, browser 1440×900 @ DPR 2.

| Check | Result |
| --- | --- |
| Production build | `tsc --noEmit && vite build` clean; JS ≈ 283.6 kB (84.2 kB gzip), CSS ≈ 42.3 kB (7.4 kB gzip) |
| SPA serving | `GET /ui/` → 200; assets load; `GET /` → 307 to `/ui/` |
| Header/status | Version `0.6.0-phase6`, phase 6, `IDLE`, `SSE LIVE`, uptime/ticks update |
| Chat round-trip | “Quanti file ci sono nella cartella public?” → “In public ci sono 11 file e una sottocartella.” with live trace |
| Trace phases | PERCEIVE → THOUGHT → PLAN → POLICY → ACT → OBSERVE → (FINAL); policy row shows rule and decision |
| Telemetry view | Host `pop-os`, kernel, 12 logical CPUs, 68 services, disks/sensors/top processes populated |
| Policy view | 9 rules (6 DENY, 1 APPROVAL, 2 ALLOW), private cap `store:private`, write roots `public · agents`, deny-list, 9-row matrix |
| Tools view | 9 tools (6 R0, 1 R1, 2 R2), capabilities and tags rendered; capability union = 8 |
| Audit view | `chain valid`, 30 records, head `254befeb…fab364`, recent decisions include approvals, failures and denials |
| Grant path | `store.mkdir` for `public/genio-demo` → modal → grant → resumed turn → honest `execution_failed` (hitech-store 403, no admin credentials), audit records `authorized` + `failed`, pending cleared |
| Deny path | `store.mkdir` for `public/genio-deny-test` → modal → deny → message “Action denied by the operator. Nothing was executed.”, `REFLECT approval denied: denied by human`, pending 0, no tool execution |
| Daemon tests | **162 passed** (`test_security` 28, `test_api` 22, `test_model_router` 21, `test_agent_loop` 17, `test_policy_engine` 25, `test_tool_registry` 49) |
| Console/browser errors | No React errors after fixes; no failed asset requests; only expected agent 500 body handled by the client |

### Known honest failure

`store.mkdir` cannot truly succeed in this environment: hitech-store requires an
admin JWT (`hitech_admin_token`) that Genio does not hold. After a valid grant the
daemon returns `execution_failed` with the store's 403 message and audits the
failure. This is the intended fail-closed result and is displayed verbatim in the
UI; granting a real write requires provisioning that credential separately.

---

## 8. Build and run

```bash
# frontend
cd apps/genio/genio_web
npm ci
npm run build

# daemon (mounts dist at /ui only if it exists at startup)
cd ../genio_server
GENIO_STATE_DIR=/tmp/opencode/genio-state \
  /tmp/opencode/genio-venv/bin/python daemon.py --port 8770
```

Open `http://127.0.0.1:8770/` → `/ui/`. For development use `npm run dev`
(`http://127.0.0.1:5173`), which proxies the daemon routes and SSE.

Reverse proxy note: preserve `/ui/`, disable proxy buffering for
`/agent/events`, and add authentication before exposing the portal off-host.
See `genio_web/README.md` for an nginx example.

---

## 9. Limitations and follow-ups

1. **Operator authentication.** The grant/deny endpoints trust any loopback
   caller. Add an authenticated operator session (or mTLS/reverse-proxy auth)
   before remote exposure.
2. **Conversation persistence.** Chat and traces are memory-only; a reload starts
   a fresh session. A daemon-side transcript store with retention policy would
   make the console resumable.
3. **Frontend automated tests.** Current verification is manual browser QA plus
   TypeScript compilation. Add Playwright coverage for the approval modal,
   deny path and SSE reconnect fallback.
4. **Policy editing.** Read-only by design in Phase 6; any future editor must
   keep the ordered-rule engine and audit trail server-side.
5. **Store write credentials.** Provision the admin token through the existing
   secret mechanism before promising real `store.mkdir` success.

---

## 10. Conclusion

Phase 6 delivers a real, self-hosted operator portal rather than a simulation.
The GUI observes the same deterministic pipeline as every other Genio client,
streams the agent's reasoning phases, and turns R2 approval into an explicit,
single-use, auditable human decision — including a safe denial path. With the
static build served by the daemon and 162 passing tests, the remaining work is
hardening (operator auth, persistence, browser tests), not core functionality.

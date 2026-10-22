# genio_web — Chronos Portal (Phase 6)

Real-time operator console for the Genio daemon. React 19 + TypeScript + Vite,
styled with Tailwind CSS v4. The daemon serves the production build at `/ui/`,
so the browser and the API share one origin: no CORS, no API keys in the client.

## What it does

| View | Purpose |
| --- | --- |
| **Console** | Chat with the agent, watch the live PERCEIVE → PLAN → POLICY → ACT → OBSERVE → REFLECT trace, and decide R2 approvals. |
| **Telemetry** | Host identity, CPU/memory history, disks, sensors, services and top processes from `/telemetry`. |
| **Policy** | Ordered P1–P9 rules, immutable deny-list, per-tool decision matrix from `/policy`. |
| **Tools** | The 9-tool registry: risk class, capability, arguments, tags and rate limit from `/tools`. |
| **Audit** | SHA-256 chain head, verification result and recent decisions from `/audit/*`. |

Privileged (R2) tool calls pause the turn and open a modal with the exact
arguments and the policy verdict. **Grant** issues a single-use,
argument-bound token and resumes the agent; **Deny** cancels the paused turn and
records the denial. Both decisions are audited by the daemon.

## Requirements

- Node.js 22+ and npm 10+
- A running Genio daemon on `127.0.0.1:8770` (default)
- For the production UI: the daemon started **after** `npm run build`, so it can
  mount `genio_web/dist`

## Develop

```bash
cd apps/genio/genio_web
npm ci
npm run dev            # http://127.0.0.1:5173
```

The Vite dev server proxies `/health`, `/telemetry`, `/agent`, `/caps`,
`/tools`, `/approvals`, `/policy`, `/portal`, `/audit`, `/memory` and `/mind`
to `http://127.0.0.1:8770` (see `vite.config.ts`). SSE works through the same
proxy. Point the proxy at another daemon, or set `VITE_GENIO_API` to an
absolute API URL when the API is exposed behind a reverse proxy.

## Build and serve

```bash
cd apps/genio/genio_web
npm run build          # tsc --noEmit && vite build → dist/

# restart the daemon so it mounts the fresh dist
cd ../genio_server
GENIO_STATE_DIR=/tmp/opencode/genio-state \
  /tmp/opencode/genio-venv/bin/python daemon.py --port 8770
```

Open `http://127.0.0.1:8770/` (307 → `/ui/`) or `http://127.0.0.1:8770/ui/`.
Assets are built with `base: "/ui/"`; serving `dist/` at a different prefix
requires changing that base or using a reverse proxy that preserves `/ui/`:

```nginx
location /ui/ {
    alias /path/to/apps/genio/genio_web/dist/;
    try_files $uri $uri/ /ui/index.html;
}
location / {
    proxy_pass http://127.0.0.1:8770;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_buffering off;          # required for /agent/events (SSE)
}
```

## Code map

```text
src/
  lib/api.ts          typed client, session bootstrap, grant/deny helpers
  lib/types.ts        daemon payload types (health, policy, tools, audit, agent)
  lib/util.ts         formatting, policy-rule copy, risk styling
  state/GenioProvider.tsx
                      single context: polling + SSE + chat queue + approvals
  components/Shell    header, status pills, sidebar navigation
  components/Console  chat, composer, live trace, approval queue, vitals
  components/ApprovalModal
                      global R2 authorization dialog (grant / deny)
  components/TelemetryView, PolicyView, ToolsView, AuditView
```

The client creates a scoped session token via `POST /caps/session` and keeps it
in `localStorage` (`genio.session.v1`); capabilities never leave the browser
except as `X-Genio-Cap` on daemon requests. Agent turns are `POST /agent/message`;
live phases arrive on `GET /agent/events` (SSE `agent`, `tick`, `tool`,
`approval`, `hello` events). If SSE reconnects mid-turn, the trace falls back to
the `steps` array returned by the POST response.

## Limitations (honest)

- **Loopback trust, no operator auth.** The daemon binds `127.0.0.1`; any local
  process can call `/approvals/{id}/grant`. The UI is a convenience and audit
  surface, not an authentication boundary. Put it behind an authenticating
  reverse proxy before exposing it beyond the host.
- **One operator, one browser session.** Chat history is in memory only and is
  lost on reload; there is no multi-user presence or server-side conversation
  store.
- **R2 actions can still fail after approval.** In this environment hitech-store
  writes require an admin token that Genio does not hold; `store.mkdir` returns
  an honest `execution_failed` after a valid grant. That is the intended
  fail-closed behaviour, not a UI error.
- **Build must exist before daemon start.** The `/ui` mount is evaluated at
  import time; rebuilding requires restarting the daemon.
- **No automated frontend tests.** Verification is manual browser QA plus the
  162 daemon tests; `tsc --noEmit` runs as part of `npm run build`.

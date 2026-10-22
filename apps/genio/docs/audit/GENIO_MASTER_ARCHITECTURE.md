# Genio — Master Architecture

**Status:** Phase 0 baseline (approved direction) · **Owner:** HiTechTN · **Last updated:** 2026-10-22
**Scope:** the sovereign, system-aware agent that acts as the *brain* of HiTech-OS (the *body*).

---

## 1. Vision

Genio is one agent with three faces over a single core:

| Face | Process | Primary use |
|---|---|---|
| **CLI / TUI** | `genio_cli` (thin client) | operator ergonomics, scripting, remote-over-SSH |
| **Daemon** | `genio_server` (systemd) | always-on agent loop, telemetry, tool execution |
| **Web GUI** | `genio_web` (browser) | dashboards, approvals, document workflows |

The daemon is the **only** component allowed to touch the system. CLI and Web are **clients** of
the daemon API — they hold no privileges, no secrets, and no tool implementations. This is the
central rule: *one brain, many faces, one enforcement point.*

### Non-goals (Phase 0)
- No autonomous privileged actions (a human approval gate is mandatory for R2 — see §6).
- No cloud dependency for core function (LLM providers are pluggable; a local model must be able
  to drive the loop, degraded but functional).
- No direct `sudo`/shell access from the agent.

---

## 2. Design principles

1. **Sovereignty** — everything runs on-prem; outbound calls are explicit, allowlisted and auditable.
2. **Least privilege** — dedicated system user, systemd sandbox, capability tokens per tool call.
3. **Deterministic core, probabilistic planner** — the LLM *proposes*; the broker *disposes*.
4. **Everything observable** — every tick, tool call, approval and failure is a structured log record.
5. **Fail-closed on privilege, fail-open on insight** — a failed probe degrades telemetry; a failed
   authorization blocks the action.
6. **No shell interpolation, ever** — tools are argv lists against an allowlist of binaries.
7. **Idempotence & reversibility** — prefer read → plan → dry-run → act → verify; R1 actions must
   have a documented rollback.

---

## 3. Topology

```
                ┌──────────────┐   ┌──────────────┐   ┌───────────────────┐
   operator ───▶│  genio_cli   │   │  genio_web   │◀──│ external agents   │
   (SSH/TTY)    │  (TUI/text)  │   │  (browser)   │   │ (hitech-store API)│
                └──────┬───────┘   └──────┬───────┘   └─────────┬─────────┘
                       │  HTTP/JSON + SSE │                     │
                       └──────────┬───────┴─────────────────────┘
                                  ▼
                   /run/genio/genio.sock  (0660 root:genio)
        ┌───────────────────────────────────────────────────────────────┐
        │                        genio-daemon                           │
        │  ┌────────────┐  ┌──────────────┐  ┌──────────┐  ┌──────────┐ │
        │  │ Agent Loop │─▶│ Tool Broker  │─▶│ Audit    │  │ Memory   │ │
        │  │ perceive→  │  │ caps+risk    │  │ JSONL    │  │ (SQLite) │ │
        │  │ plan→act→  │  │ allowlist    │  │ hash-    │  │          │ │
        │  │ observe    │  │ approval     │  │ chained  │  │          │ │
        │  └─────┬──────┘  └──────┬───────┘  └──────────┘  └──────────┘ │
        │        │                │                                     │
        │  ┌─────▼────────────────▼───────────────────────────────────┐  │
        │  │ Telemetry Collector (system_monitor) — read-only probes  │  │
        │  └─────┬───────────┬──────────┬────────────┬────────────────┘  │
        └────────┼───────────┼──────────┼────────────┼───────────────────┘
                 ▼           ▼          ▼            ▼
              /proc,sys   systemctl   journald     hitech-store API
              (psutil)    (D-Bus)     (read)       (documents, XSS-safe HTML)
                 │           │          │            │
                 └───────────┴──────────┴────────────┴──▶ HiTech-OS body
                                   ▲
                     mosquitto (MQTT events) · telegraf → influxdb2 (metrics)
```

**Why a Unix socket:** no TCP port exposed by default; filesystem permissions are the ACL;
`systemd` socket activation gives on-demand start without a supervisor of our own. A
`127.0.0.1:8765` dev listener exists **only** in development runs (explicit env flag).

---

## 4. Components

| Module | Path | Phase | Responsibility | Privilege |
|---|---|---|---|---|
| Daemon/API | `genio_server/daemon.py` | 1 ✅ | HTTP API, agent loop lifecycle, state | genio |
| Telemetry | `genio_server/core/system_monitor.py` | 1 ✅ | read-only OS snapshot (CPU/RAM/disk/services/procs) | genio (unprivileged) |
| Tool Broker | `genio_server/core/broker.py` | 2 | capability check → allowlist exec → audit | genio |
| Capability tokens | `genio_server/core/caps.py` | 2 | issue/verify HMAC tokens, scopes, TTL, nonce | daemon only |
| Memory | `genio_server/core/memory.py` | 3 | sessions, facts, embeddings (SQLite WAL) | genio |
| Planner | `genio_server/core/planner.py` | 3 | LLM adapter (local-first, cloud opt-in) | none |
| Approval gate | daemon endpoint + web UI | 4 | human sign-off for R2 actions | admin |
| CLI/TUI | `genio_cli/` | 2 | client (textual/rich) | user |
| Web GUI | `genio_web/` | 4 | SPA client + SSE stream | user |
| NixOS module | `os/services/genio.nix` | 5 | user, sandbox, socket, secrets | root (deploy) |

---

## 5. Interfaces

### 5.1 Daemon API (v0, Phase 1)

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | liveness, version, uptime, loop ticks |
| GET | `/telemetry` | latest snapshot (`?fresh=true` bypasses cache) |
| GET | `/agent/status` | tick counter, last tick, error count, ring buffer |

Phase 2 adds `POST /agent/message` (SSE stream), `GET /tools`, `POST /tools/{id}/invoke`,
`POST /approvals/{id}`.

### 5.2 hitech-store (document plane)

Genio never parses PDF/DOCX itself. It uses the store as the **document plane**:

```
GET /api/drive/list?path=<dir>            → inventory (name, size, sha256, kind)
GET /api/drive/extract?path=<file>        → {kind, html, truncated}  ← XSS-safe HTML
```

Rules:
- `public/*` is readable anonymously; `private/*` and `agents/*` require `X-Agent-Token` or an
  admin session cookie. Genio will use the **agent token** channel (shared secret), never the
  admin session.
- Extracted HTML is **untrusted input**: it is sanitized to text (tags stripped) before entering
  the LLM context; the raw HTML is only ever rendered in a sandboxed iframe by the web UI.
- Uploads from Genio go to `agents/genio/*` so they are attributable and revocable.

### 5.3 OS surfaces

| Surface | Access | Notes |
|---|---|---|
| `/proc`, `/sys` | read | psutil with `/proc` fallback |
| systemd | `systemctl` (read verbs) | Phase 2 adds `start/stop/restart` behind R2 approval |
| journald | `journalctl --since` (read) | bounded `-n`, JSON output |
| filesystem | scoped roots | R1 writes only under `/var/lib/genio` + allowlisted export dirs |
| MQTT | subscribe `hitech/#` (Phase 3) | event ingestion from other services |
| InfluxDB | query (Phase 3) | historical metrics for trend questions |

---

## 6. Trust boundaries & capability model

```
 TB0  untrusted content (prompts, documents, web/API input)
  │      — never executed, never trusted as instructions
  ▼
 TB1  genio-daemon (agent runtime)         user=genio, sandboxed, no root
  │      — holds session, plans, calls broker with *intent*
  ▼
 TB2  tool broker (choke point)            same process, isolated module
  │      — verifies capability token + risk class + allowlist + rate limit
  ▼
 TB3  privileged operations                root-owned helpers / systemd units
  │      — only via pre-defined units or polkit actions; never raw root shell
  ▼
 TB4  secrets                              /run/secrets/genio.env (0640 root:genio)
         — never logged, never sent to LLM, redacted in error paths
```

### 6.1 Capability tokens

```jsonc
{ "sub": "session:<uuid>", "caps": ["telemetry:read", "service:read", "store:extract"],
  "iat": 1730000000, "exp": 1730000600, "nonce": "<random>" }   // HMAC-SHA256, daemon key
```

- Issued by the daemon **per session**, narrowest scope that satisfies the declared intent.
- Verified on every broker call (constant-time compare, TTL, nonce replay cache).
- Tokens are **not** user credentials: a human authenticates once (OTP/local login), the daemon
  mints capabilities; the CLI/Web never see the system directly.

### 6.2 Risk classes

| Class | Examples | Gate |
|---|---|---|
| **R0 read-only** | telemetry, list services, read file in scope, extract document | automatic |
| **R1 reversible write** | write file under `/var/lib/genio`, create share link, enqueue job | automatic + audit + rate limit |
| **R2 privileged** | restart/stop unit, install package, modify network, delete data | **human approval** (signed intent, TTL, single-use) |

Every R1/R2 action carries `intent` (human-readable), `session`, `cap`, `argv`, `result`, and a
hash-chain link to the previous audit record.

### 6.3 Executor invariants

1. `subprocess.run(argv, shell=False)` with `check=False`, `timeout`, minimal env (`PATH`, `LANG=C`).
2. Binary resolved from an allowlist (`systemctl`, `journalctl`, `df`, `ss`, `ip -br`, …).
3. Path arguments resolved with `Path.resolve()` and verified under an allowed root.
4. Unit arguments verified against a pattern (`^[a-zA-Z0-9@._-]+\.service$`) and a deny-list
   (`genio.service` itself, `sshd`, boot-critical units).
5. Rate limit: N calls/minute per capability; hard stop on tripped circuit breaker.

---

## 7. Agent loop

```
        ┌─────────────────────────── every GENIO_TICK_SECONDS ─────────────────────────┐
        │                                                                              │
   perceive ──▶ plan ──▶ authorize ──▶ act ──▶ observe ──▶ reflect ──▶ (sleep)        │
   (telemetry)  (LLM or          (broker:      (tool)      (diff vs     (update memory, │
                 rules)           caps/risk)                expectation)  emit event)   │
        │                                                                              │
        └──────────────── max_steps / time budget / cancel token ─────────────────────┘
```

- **Bounded:** `max_steps` (default 8), wall-clock budget, token budget; the loop exits to idle.
- **Cancellable:** `POST /agent/cancel` sets an asyncio event; the current step aborts between tools.
- **Deterministic fallback:** if the planner is unavailable, the loop only *observes* and reports —
  it never improvises actions.
- **Observability:** each tick emits one structured log line and appends to a ring buffer
  (60 samples) exposed by `/agent/status`. Phase 3 persists to SQLite.

---

## 8. Deployment on HiTech-OS

```nix
# os/services/genio.nix (Phase 5, sketch)
systemd.services.genio = {
  serviceConfig = {
    User = "genio"; DynamicUser = false;
    ExecStart = "${genioDaemon}/bin/genio-daemon";
    RuntimeDirectory = "genio";            # /run/genio, 0750 root:genio
    StateDirectory = "genio";              # /var/lib/genio
    EnvironmentFile = "/run/secrets/genio.env";
    NoNewPrivileges = true; ProtectSystem = "strict"; ProtectHome = true;
    PrivateTmp = true; PrivateDevices = true; ProtectKernelTunables = true;
    RestrictAddressFamilies = [ "AF_UNIX" ];   # + AF_INET only if a cloud LLM is enabled
    MemoryMax = "512M"; CPUQuota = "50%";
  };
};
```

Telemetry probes need **no** extra privilege. R2 actions are delegated to dedicated root-owned
units/polkit actions so the daemon itself never runs as root (blast-radius containment).

---

## 9. Degradation matrix

| Failure | Effect | Behaviour |
|---|---|---|
| psutil missing | no CPU/RAM | `/proc` fallback; snapshot still valid |
| systemctl timeout (2 s) | services unknown | `services: null`, others unaffected |
| hitech-store down | no document extraction | tool returns `unavailable`; agent reports, no retry storm |
| LLM provider down | no planning | observe-only mode; explicit `degraded: true` in `/health` |
| audit log unwritable | **all** R1/R2 blocked | fail-closed; telemetry keeps running |

---

## 10. Roadmap

| Phase | Deliverable | Acceptance |
|---|---|---|
| **0** ✅ | scaffold + this blueprint | tree exists, decisions recorded |
| **1** ✅ | daemon + telemetry + health API + loop skeleton | `/health`, `/telemetry`, ticks observable |
| 2 | tool broker + capability tokens + CLI | R0 tools via CLI, R2 denied without approval |
| 3 | memory + planner + MQTT/journal ingestion | multi-turn session, facts persisted |
| 4 | web GUI + approval workflow | browser approval of an R2 action, audited |
| 5 | NixOS module + package + secrets | `systemctl start genio`, sandbox verified |
| 6 | hitech-store agent channel + document Q&A | Arabic PDF/DOCX Q&A with citations |

---

## 11. Architecture decisions (ADR log)

| # | Decision | Rationale | Consequence |
|---|---|---|---|
| 0001 | **Daemon-first**, CLI/Web are clients | one enforcement point, no duplicated privilege | daemon must be up for any action |
| 0002 | **Unix socket** transport by default | OS-level ACL, no exposed port | remote access via SSH forwarding only |
| 0003 | **Capability tokens** instead of sudo/shell | per-call scope, revocable, auditable | broker complexity; must be tested hard |
| 0004 | **LLM plans, broker executes** | prompt injection cannot become command injection | tool surface must be small and typed |
| 0005 | **hitech-store is the document plane** | reuse hardened, XSS-safe extraction; no duplicate parsers | dependency on store availability |
| 0006 | **Telemetry read-only in-process** | cheap, no privilege, no agent on host | no kernel-level tracing in Phase 1 |

---

## 12. Open risks

1. **Prompt injection via documents** — mitigated by text-sanitization + capability scope, but the
   planner can still be *persuaded* to request R2 actions; the human gate is the backstop.
2. **Audit tampering** — hash chaining is planned; the log must live outside the daemon's write
   scope (root-owned dir, append-only) to be meaningful.
3. **Token leakage to LLM** — capabilities never enter prompts; redaction tests required in Phase 2.
4. **Resource abuse** — a runaway loop can starve the VM; enforced via systemd limits + budgets.

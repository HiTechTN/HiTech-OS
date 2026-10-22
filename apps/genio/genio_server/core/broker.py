"""Tool broker — the single choke point for every Genio action.

Risk classes (see docs/audit/GENIO_MASTER_ARCHITECTURE.md §6.2):

  R0  read-only            → auto-approved
  R1  reversible write     → auto-approved, rate-limited, audited
  R2  privileged/destructive → requires a single-use *signed human approval*
                              bound to the exact tool id + arguments hash

Audit invariant: the log is a SHA-256 hash chain (``prev`` links). If the log is
not writable, every R1/R2 invocation is refused **before** any side effect
(fail-closed). R0 keeps working so the agent can still observe.

Executor invariants: allowlisted argv, ``shell=False``, timeouts, path resolution
under an allowed root, unit regex + deny-list.
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
import re
import subprocess
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from enum import Enum
from pathlib import Path
from typing import Any, Callable

from . import caps, system_monitor

log = logging.getLogger("genio.broker")

GENESIS = "0" * 64
UNIT_RE = re.compile(r"^[A-Za-z0-9@._:-]+\.(service|timer|socket)$")
DENY_UNITS = {
    "genio.service", "ssh.service", "sshd.service", "dbus.service",
    "systemd-logind.service", "systemd-journald.service", "polkit.service",
}
DENY_UNIT_PREFIXES = ("systemd-", "user@", "getty@")
MIN_ENV = {
    "PATH": os.environ.get("PATH", "/run/current-system/sw/bin:/usr/bin:/bin"),
    "LANG": "C", "LC_ALL": "C",
}


class Risk(str, Enum):
    R0 = "R0"
    R1 = "R1"
    R2 = "R2"


class ToolArgError(ValueError):
    """Invalid or unsafe tool arguments."""


# --------------------------------------------------------------------------- audit
class AuditUnavailable(RuntimeError):
    """The audit log cannot be written — R1/R2 must fail closed."""


class AuditLog:
    """Append-only JSONL log with a SHA-256 hash chain."""

    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        self._lock = threading.Lock()
        self._seq = 0
        self._head = GENESIS
        self._load_head()

    # -- internals ---------------------------------------------------------
    def _load_head(self) -> None:
        try:
            with self.path.open("rb") as fh:
                last = None
                for line in fh:
                    if line.strip():
                        last = line
                if last:
                    rec = json.loads(last)
                    self._seq = int(rec.get("seq", 0))
                    self._head = rec.get("hash", GENESIS)
        except (OSError, ValueError, json.JSONDecodeError):
            return  # missing/empty log → genesis

    @staticmethod
    def _hash(prev: str, body: dict[str, Any]) -> str:
        material = prev + "|" + json.dumps(body, sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(material.encode()).hexdigest()

    # -- public ------------------------------------------------------------
    def ensure_writable(self) -> bool:
        """Best-effort check: directory exists/creatable and the file is writable."""
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            if self.path.exists():
                return os.access(self.path, os.W_OK)
            with self.path.open("a", encoding="utf-8"):
                pass
            return os.access(self.path, os.W_OK)
        except OSError:
            return False

    def append(self, *, actor: str, tool: str, risk: str, args_hash: str,
               decision: str, detail: str = "", extra: dict[str, Any] | None = None) -> int:
        with self._lock:
            if not self.ensure_writable():
                raise AuditUnavailable("audit log is not writable")
            self._seq += 1
            body: dict[str, Any] = {
                "seq": self._seq,
                "ts": datetime.now(timezone.utc).isoformat(timespec="seconds"),
                "actor": actor,
                "tool": tool,
                "risk": risk,
                "args_hash": args_hash,
                "decision": decision,
                "detail": detail[:500],
                "prev": self._head,
            }
            if extra:
                body["extra"] = extra
            digest = self._hash(self._head, body)
            line = json.dumps({**body, "hash": digest}, ensure_ascii=False)
            try:
                with self.path.open("a", encoding="utf-8") as fh:
                    fh.write(line + "\n")
                    fh.flush()
                    os.fsync(fh.fileno())
            except OSError as exc:
                raise AuditUnavailable(str(exc)) from exc
            self._head = digest
            return self._seq

    def verify_chain(self) -> dict[str, Any]:
        prev = GENESIS
        count = 0
        try:
            lines = self.path.read_text(encoding="utf-8").splitlines()
        except OSError as exc:
            return {"ok": False, "records": 0, "error": f"unreadable: {exc}"}
        for i, line in enumerate(lines, 1):
            if not line.strip():
                continue
            try:
                rec = json.loads(line)
                claimed = rec.pop("hash")
            except (ValueError, KeyError) as exc:
                return {"ok": False, "records": count, "error": f"malformed record at line {i}: {exc}"}
            if rec.get("prev") != prev:
                return {"ok": False, "records": count,
                        "error": f"broken chain at seq {rec.get('seq')} (line {i})"}
            if self._hash(prev, rec) != claimed:
                return {"ok": False, "records": count,
                        "error": f"hash mismatch at seq {rec.get('seq')} (line {i})"}
            prev = claimed
            count += 1
        return {"ok": True, "records": count, "head": prev}

    @property
    def head(self) -> str:
        return self._head

    @property
    def seq(self) -> int:
        return self._seq


# --------------------------------------------------------------------------- approvals
@dataclass
class PendingApproval:
    id: str
    tool: str
    args_hash: str
    session: str
    intent: str
    created_at: float
    expires_at: float
    risk: str = Risk.R2.value
    granted: bool = False


class ApprovalStore:
    """Pending R2 requests awaiting a human decision."""

    def __init__(self, ttl_s: int = 300) -> None:
        self.ttl_s = ttl_s
        self._pending: dict[str, PendingApproval] = {}
        self._lock = threading.Lock()

    def _purge(self) -> None:
        now = time.time()
        for pid, item in list(self._pending.items()):
            if item.expires_at < now:
                self._pending.pop(pid, None)

    def request(self, *, tool: str, args_hash_val: str, session: str, intent: str) -> PendingApproval:
        import secrets
        with self._lock:
            self._purge()
            item = PendingApproval(
                id="apr_" + secrets.token_hex(8),
                tool=tool,
                args_hash=args_hash_val,
                session=session,
                intent=intent,
                created_at=time.time(),
                expires_at=time.time() + self.ttl_s,
            )
            self._pending[item.id] = item
            return item

    def get(self, approval_id: str) -> PendingApproval | None:
        with self._lock:
            self._purge()
            return self._pending.get(approval_id)

    def list(self) -> list[dict[str, Any]]:
        with self._lock:
            self._purge()
            return [
                {"id": p.id, "tool": p.tool, "intent": p.intent, "risk": p.risk,
                 "expires_in_s": max(0, round(p.expires_at - time.time()))}
                for p in self._pending.values() if not p.granted
            ]

    def grant(self, approval_id: str, approver: str, ttl_s: int = 120) -> str:
        """Human decision → single-use, args-bound approval token."""
        with self._lock:
            self._purge()
            item = self._pending.get(approval_id)
            if not item:
                raise KeyError("unknown or expired approval request")
            if not item.granted:
                item.granted = True
            # keep the request until its TTL so a retry reports "granted" clearly
        return caps.issue(
            subject=f"approval:{approver}",
            caps=[f"approval:{item.tool}"],
            ttl_s=ttl_s,
            kind="approval",
            tool=item.tool,
            args_hash=item.args_hash,
            meta={"approval_id": item.id, "approver": approver, "intent": item.intent},
        )

    def deny(self, approval_id: str, approver: str = "local-human") -> dict[str, Any]:
        """Discard a pending request without issuing any token."""
        with self._lock:
            self._purge()
            item = self._pending.pop(approval_id, None)
            if not item:
                raise KeyError("unknown or expired approval request")
            return {"id": item.id, "tool": item.tool, "session": item.session,
                    "intent": item.intent, "approver": approver}


# --------------------------------------------------------------------------- tools
@dataclass(frozen=True)
class ToolSpec:
    id: str
    risk: Risk
    description: str
    cap: str
    params: dict[str, dict[str, Any]]
    handler: Callable[[dict[str, Any]], dict[str, Any]]
    rate_per_min: int = 60
    summary: Callable[[dict[str, Any]], str] = lambda a: ""
    # Semantic validation that must run *before* the risk gate, so a human is
    # never asked to approve an action whose arguments are already illegal.
    validator: Callable[[dict[str, Any]], dict[str, Any]] | None = None
    # Catalogue grouping (system / store / documents / media / directories).
    tags: tuple[str, ...] = ()


def _run(argv: list[str], timeout: float = 5.0) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=timeout,
                              check=False, env=MIN_ENV)
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except (OSError, subprocess.TimeoutExpired) as exc:
        return -1, "", f"{type(exc).__name__}: {exc}"


def _systemctl(*args: str, timeout: float = 5.0) -> tuple[int, str, str]:
    binary = "/run/current-system/sw/bin/systemctl" if Path("/run/current-system/sw/bin/systemctl").exists() else "systemctl"
    # Never block on a polkit agent: fail fast instead of hanging a worker.
    return _run([binary, "--no-ask-password", *args], timeout)


def _check_unit(unit: str) -> str:
    unit = (unit or "").strip()
    if not UNIT_RE.match(unit):
        raise ToolArgError("invalid unit name")
    if unit in DENY_UNITS or unit.startswith(DENY_UNIT_PREFIXES):
        raise ToolArgError("unit is protected")
    return unit


def _resolve_under(base: Path, rel: str) -> Path:
    """Path traversal guard: the resolved target must stay inside ``base``."""
    rel = (rel or "").strip()
    if not rel or len(rel) > 240 or "\x00" in rel:
        raise ToolArgError("invalid path")
    base_r = base.resolve()
    target = (base_r / rel).resolve()
    try:
        target.relative_to(base_r)
    except ValueError as exc:
        raise ToolArgError("path escapes the allowed root") from exc
    if target == base_r:
        raise ToolArgError("path must name a file")
    return target


def system_tools(state_dir: Path) -> list[ToolSpec]:
    """Local (non-store) tools: telemetry, services, sandboxed writes, restarts."""
    files_root = (Path(state_dir) / "files").resolve()

    def t_telemetry(_: dict[str, Any]) -> dict[str, Any]:
        return system_monitor.snapshot(force=True)

    def t_services(_: dict[str, Any]) -> dict[str, Any]:
        rc, out, err = _systemctl("list-units", "--type=service", "--no-legend",
                                  "--plain", "--no-pager", "--state=running")
        if rc != 0:
            raise RuntimeError(err or "systemctl failed")
        units = [ln.split()[0] for ln in out.splitlines() if ln.strip()]
        rcf, outf, errf = _systemctl("list-units", "--type=service", "--no-legend",
                                     "--plain", "--no-pager", "--state=failed")
        failed = [ln.split()[0] for ln in outf.splitlines() if ln.strip()] if rcf == 0 else []
        return {"running_count": len(units), "failed": failed[:50], "sample": units[:50]}

    def t_service_status(args: dict[str, Any]) -> dict[str, Any]:
        unit = _check_unit(args["unit"])
        rc, out, err = _systemctl("is-active", unit)
        rc2, vals, err2 = _systemctl("show", unit, "-p", "ActiveState,SubState,MainPID", "--value")
        props = dict(zip(("active_state", "sub_state", "main_pid"), vals.splitlines()))
        return {"unit": unit, "is_active_rc": rc, "stdout": out,
                "stderr": err, **props}

    def v_path(args: dict[str, Any]) -> dict[str, Any]:
        _resolve_under(files_root, args["path"])
        return args

    def v_unit(args: dict[str, Any]) -> dict[str, Any]:
        _check_unit(args["unit"])
        return args

    def t_file_write(args: dict[str, Any]) -> dict[str, Any]:
        target = _resolve_under(files_root, args["path"])
        target.parent.mkdir(parents=True, exist_ok=True)
        payload = args["content"].encode()
        target.write_bytes(payload)
        return {"path": str(target), "bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest()}

    def t_service_restart(args: dict[str, Any]) -> dict[str, Any]:
        unit = _check_unit(args["unit"])
        rc, out, err = _systemctl("restart", unit, timeout=8.0)
        if rc != 0:
            # Be honest: an authorised action that the OS refused (polkit,
            # read-only fs, missing unit) is a *failure*, not an execution.
            raise RuntimeError(f"systemctl restart {unit} failed (rc={rc}): {err or 'no output'}")
        return {"unit": unit, "rc": rc, "stdout": out, "stderr": err}

    return [
        ToolSpec("telemetry.snapshot", Risk.R0, "Full read-only system telemetry snapshot.",
                 "telemetry:read", {}, t_telemetry, 240, lambda a: "system"),
        ToolSpec("services.list", Risk.R0, "List running and failed systemd services.",
                 "service:read", {}, t_services, 120, lambda a: "systemd"),
        ToolSpec("service.status", Risk.R0, "Status of one systemd unit.",
                 "service:read", {"unit": {"type": "str", "required": True}},
                 t_service_status, 120, lambda a: a.get("unit", ""), v_unit,
                 tags=("system",)),
        ToolSpec("file.write", Risk.R1, "Write a text file under the Genio state dir.",
                 "fs:write", {"path": {"type": "str", "required": True},
                              "content": {"type": "str", "required": True}},
                 t_file_write, 10, lambda a: a.get("path", ""), v_path,
                 tags=("system", "write")),
        ToolSpec("service.restart", Risk.R2, "Restart a systemd unit (privileged).",
                 "service:restart", {"unit": {"type": "str", "required": True}},
                 t_service_restart, 3, lambda a: a.get("unit", ""), v_unit,
                 tags=("system", "privileged")),
    ]


def default_tools(state_dir: Path) -> list[ToolSpec]:
    """Full default catalogue: system tools + hitech-store tools.

    Lazy import avoids a module cycle: ``tool_registry`` imports ``ToolSpec``
    from here while this function asks the registry to compose the catalogue.
    """
    from .tool_registry import build_default_registry

    return build_default_registry(state_dir).all()


# --------------------------------------------------------------------------- broker
class Broker:
    def __init__(self, state_dir: Path | str, tools: list[ToolSpec] | None = None,
                 approval_ttl_s: int = 300, policy: Any | None = None) -> None:
        self.state_dir = Path(state_dir)
        self.policy = policy
        self.audit = AuditLog(self.state_dir / "audit" / "genio-audit.jsonl")
        self.approvals = ApprovalStore(ttl_s=approval_ttl_s)
        self._tools: dict[str, ToolSpec] = {t.id: t for t in (tools or default_tools(self.state_dir))}
        self._rate: dict[tuple[str, str], list[float]] = {}
        self._lock = threading.Lock()

    # -- introspection -----------------------------------------------------
    def tools(self) -> list[dict[str, Any]]:
        return [
            {"id": t.id, "risk": t.risk.value, "description": t.description,
             "cap": t.cap, "tags": list(t.tags), "rate_per_min": t.rate_per_min,
             "params": t.params, "requires_approval": t.risk is Risk.R2}
            for t in sorted(self._tools.values(), key=lambda x: (x.risk.value, x.id))
        ]

    def get_tool(self, tool_id: str) -> ToolSpec | None:
        return self._tools.get(tool_id)

    # -- helpers -----------------------------------------------------------
    @staticmethod
    def _deny(code: str, message: str, **extra: Any) -> dict[str, Any]:
        return {"ok": False, "decision": "denied", "code": code, "message": message, **extra}

    def _validate(self, tool: ToolSpec, args: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(args, dict):
            raise ToolArgError("args must be an object")
        cleaned: dict[str, Any] = {}
        for name, spec in tool.params.items():
            if name in args:
                value = args[name]
                if spec.get("type") == "str" and not isinstance(value, str):
                    raise ToolArgError(f"{name} must be a string")
                if spec.get("type") == "int" and not isinstance(value, int):
                    raise ToolArgError(f"{name} must be an integer")
                cleaned[name] = value
            elif spec.get("required"):
                raise ToolArgError(f"missing required argument {name!r}")
        if tool.validator is not None:
            cleaned = tool.validator(cleaned)
        return cleaned

    def _rate_ok(self, tool: ToolSpec, subject: str) -> bool:
        now = time.time()
        key = (tool.id, subject)
        with self._lock:
            window = [t for t in self._rate.get(key, []) if now - t < 60]
            if len(window) >= tool.rate_per_min:
                self._rate[key] = window
                return False
            window.append(now)
            self._rate[key] = window
            return True

    # -- invocation --------------------------------------------------------
    def invoke(self, tool_id: str, args: dict[str, Any] | None,
               session_token: str, approval_token: str | None = None) -> dict[str, Any]:
        args = args or {}
        tool = self._tools.get(tool_id)
        if not tool:
            return self._deny("unknown_tool", f"unknown tool {tool_id!r}")

        # 1. session capability
        try:
            session = caps.verify(session_token, tool.cap, consume=False)
        except caps.CapError as exc:
            return self._deny(type(exc).__name__.lower(), f"session rejected: {exc}")
        subject = str(session.get("sub", "unknown"))

        # 2. arguments
        try:
            clean = self._validate(tool, args)
            args_hash = caps.hash_args(clean)
            summary = tool.summary(clean)
        except ToolArgError as exc:
            self._audit_safe(subject, tool, "", "denied", f"args: {exc}")
            return self._deny("invalid_args", str(exc))

        # 3. rate limit
        if not self._rate_ok(tool, subject):
            self._audit_safe(subject, tool, args_hash, "rate_limited", summary)
            return self._deny("rate_limited", f"rate limit {tool.rate_per_min}/min exceeded")

        # 3.5 deterministic policy: capabilities and scope, never model opinion
        if self.policy is not None:
            verdict = self.policy.evaluate(tool, subject, session.get("caps") or [], clean)
            if verdict.decision == "DENY":
                self._audit_safe(subject, tool, args_hash, "denied",
                                 f"policy:{verdict.code}: {verdict.reason}")
                return self._deny(verdict.code, verdict.reason, rule=verdict.rule)

        # 4. audit must be writable before any R1/R2 side effect (fail-closed)
        if tool.risk in (Risk.R1, Risk.R2) and not self.audit.ensure_writable():
            return self._deny("audit_unavailable",
                              "audit log is not writable — privileged actions are frozen")

        # 5. R2 → signed, args-bound, single-use human approval
        approver = ""
        if tool.risk is Risk.R2:
            if not approval_token:
                pending = self.approvals.request(
                    tool=tool.id, args_hash_val=args_hash, session=subject,
                    intent=f"{tool.id} → {summary or '(no target)'}",
                )
                self._audit_safe(subject, tool, args_hash,
                                 "approval_requested", pending.intent)
                return {
                    "ok": False, "decision": "approval_required",
                    "code": "approval_required",
                    "message": "This action is privileged. A human must approve it.",
                    "approval_id": pending.id,
                    "approval_prompt": f"Approve {tool.description} [{summary}] ?",
                    "risk": tool.risk.value,
                    "expires_in_s": self.approvals.ttl_s,
                }
            try:
                approval = caps.verify(approval_token, f"approval:{tool.id}")
            except caps.ReplayDetected:
                self._audit_safe(subject, tool, args_hash, "denied", "approval replay")
                return self._deny("approval_replayed", "approval token already used")
            except caps.CapError as exc:
                self._audit_safe(subject, tool, args_hash, "denied", f"approval: {exc}")
                return self._deny("approval_invalid", f"approval rejected: {exc}")
            if approval.get("kind") != "approval" or approval.get("tool") != tool.id:
                return self._deny("approval_invalid", "approval is not bound to this tool")
            if approval.get("args_hash") != args_hash:
                return self._deny("approval_mismatch", "approval is bound to different arguments")
            approver = str((approval.get("meta") or {}).get("approver", "human"))

        # 6. pre-execution audit record (R1/R2 mandatory)
        if tool.risk in (Risk.R1, Risk.R2):
            try:
                self.audit.append(actor=subject, tool=tool.id, risk=tool.risk.value,
                                  args_hash=args_hash, decision="authorized",
                                  detail=summary, extra={"approver": approver} if approver else None)
            except AuditUnavailable as exc:
                return self._deny("audit_unavailable", f"audit append failed: {exc}")

        # 7. execute
        try:
            result = tool.handler(clean)
            outcome, detail = "executed", summary
            ok = True
            error = None
        except ToolArgError as exc:
            outcome, detail, ok, error = "failed", f"args: {exc}", False, str(exc)
            result = None
        except Exception as exc:  # handler explosion is contained here
            outcome, detail, ok, error = "failed", f"{type(exc).__name__}: {exc}", False, str(exc)
            result = None
            log.warning("tool %s failed: %s", tool.id, exc)

        # 8. post-execution audit
        seq = None
        if tool.risk in (Risk.R1, Risk.R2):
            try:
                seq = self.audit.append(actor=subject, tool=tool.id, risk=tool.risk.value,
                                        args_hash=args_hash, decision=outcome,
                                        detail=detail,
                                        extra={"error": error} if error else None)
            except AuditUnavailable:
                log.error("post-execution audit failed for %s (seq %s)", tool.id, seq)
        else:
            self._audit_safe(subject, tool, args_hash, outcome, detail)

        if not ok:
            return {"ok": False, "decision": "failed", "code": "execution_failed",
                    "tool": tool.id, "risk": tool.risk.value, "message": error,
                    "audit_seq": seq}
        return {"ok": True, "decision": "executed", "tool": tool.id,
                "risk": tool.risk.value, "result": result, "audit_seq": seq}

    def record_denial(self, tool_id: str, args: dict[str, Any], subject: str,
                      code: str, reason: str) -> int | None:
        """Audit a denial that happened *before* execution (policy layer)."""
        tool = self._tools.get(tool_id)
        if tool is None:
            return None
        try:
            args_hash = caps.hash_args(args or {})
        except Exception:
            args_hash = ""
        try:
            return self.audit.append(actor=subject, tool=tool.id, risk=tool.risk.value,
                                     args_hash=args_hash, decision="denied",
                                     detail=f"policy:{code}: {reason}"[:500])
        except AuditUnavailable:
            return None

    def _audit_safe(self, actor: str, tool: ToolSpec, args_hash: str,
                    decision: str, detail: str) -> None:
        """Best-effort audit for denials and R0 (never raises)."""
        try:
            self.audit.append(actor=actor, tool=tool.id, risk=tool.risk.value,
                              args_hash=args_hash, decision=decision, detail=detail)
        except AuditUnavailable:
            log.warning("audit unavailable while recording %s/%s", tool.id, decision)


def build_default_broker(state_dir: Path | str | None = None) -> Broker:
    root = Path(state_dir or os.getenv("GENIO_STATE_DIR", "/var/lib/genio"))
    return Broker(root)

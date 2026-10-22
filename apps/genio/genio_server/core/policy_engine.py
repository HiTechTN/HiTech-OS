"""Deterministic policy engine.

The LLM may *propose* a tool call, but it is never asked whether it is allowed
to. This module answers that question from facts only:

    tool metadata (risk, required capability)  ← the registry
    session capabilities                       ← the signed token / caller
    arguments (store path, mkdir target)       ← validated, never trusted

Decisions are a closed set:

    ALLOW                 execute (R0/R1, capability present)
    DENY                  refuse, with a stable machine code
    REQUIRE_CONFIRMATION  pause for a human (R2)

Rules are ordered and non-overlapping; the first matching rule wins, so the
result is reproducible for identical inputs. There is no scoring, no model
call, and no randomness.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any, Iterable

from .broker import Risk, ToolSpec

PRIVATE_CAP = "store:private"
WRITE_ROOTS = ("public", "agents")


class Decision(str, Enum):
    ALLOW = "ALLOW"
    DENY = "DENY"
    REQUIRE_CONFIRMATION = "REQUIRE_CONFIRMATION"


@dataclass(frozen=True)
class PolicyDecision:
    decision: Decision
    code: str
    reason: str
    tool: str
    risk: str
    capability: str
    rule: str

    @property
    def allowed(self) -> bool:
        return self.decision is Decision.ALLOW

    @property
    def denied(self) -> bool:
        return self.decision is Decision.DENY

    @property
    def requires_confirmation(self) -> bool:
        return self.decision is Decision.REQUIRE_CONFIRMATION

    def as_dict(self) -> dict[str, Any]:
        return {"decision": self.decision.value, "code": self.code,
                "reason": self.reason, "tool": self.tool, "risk": self.risk,
                "capability": self.capability, "rule": self.rule}


@dataclass(frozen=True)
class PolicyRule:
    id: str
    decision: str
    description: str

    def as_dict(self) -> dict[str, str]:
        return {"id": self.id, "decision": self.decision,
                "description": self.description}


DEFAULT_RULES: tuple[PolicyRule, ...] = (
    PolicyRule("P1_unknown_tool", "DENY",
               "Tool id is not present in the registry."),
    PolicyRule("P2_deny_list", "DENY",
               "Tool is on the hard deny-list regardless of capabilities."),
    PolicyRule("P3_subject_blocked", "DENY",
               "Subject (session identity) is blocked."),
    PolicyRule("P4_missing_capability", "DENY",
               "Signed session token does not carry the tool's capability."),
    PolicyRule("P5_private_store", "DENY",
               "Path under private/ requires the store:private capability."),
    PolicyRule("P6_write_scope", "DENY",
               "Writes are only allowed under public/ or agents/."),
    PolicyRule("P7_r2_confirmation", "REQUIRE_CONFIRMATION",
               "R2 actions require a single-use human approval token."),
    PolicyRule("P8_r1_allow", "ALLOW",
               "R1 writes are allowed with mandatory audit."),
    PolicyRule("P9_r0_allow", "ALLOW",
               "R0 reads are allowed."),
)

DEFAULT_DENY_TOOLS = frozenset({
    "store.delete", "store.purge", "store.rmdir", "shell.exec", "system.reboot",
})


class PolicyEngine:
    """Stateless, deterministic evaluator (safe to share across threads)."""

    def __init__(self, deny_tools: Iterable[str] | None = None,
                 deny_subjects: Iterable[str] | None = None,
                 rules: tuple[PolicyRule, ...] = DEFAULT_RULES) -> None:
        self.deny_tools = frozenset(deny_tools if deny_tools is not None else DEFAULT_DENY_TOOLS)
        self.deny_subjects = frozenset(deny_subjects or ())
        self.rules = rules

    # ------------------------------------------------------------------ api
    def evaluate(self, tool: ToolSpec | None, subject: str,
                 capabilities: Iterable[str],
                 args: dict[str, Any] | None = None) -> PolicyDecision:
        caps = set(capabilities or ())
        # `args is None` means a risk-only what-if (matrix view); concrete
        # invocations always pass their arguments so path rules can be checked.
        risk_only = args is None
        args = args or {}
        tool_id = getattr(tool, "id", "<unknown>")
        risk = getattr(getattr(tool, "risk", None), "value", "?")
        cap = getattr(tool, "cap", "")

        # P1 — unknown tool
        if tool is None:
            return self._d(Decision.DENY, "unknown_tool", "unknown tool id",
                           tool_id, risk, cap, "P1_unknown_tool")

        # P2 — hard deny-list (capabilities cannot unlock these)
        if tool.id in self.deny_tools:
            return self._d(Decision.DENY, "denied_by_policy",
                           f"{tool.id} is disabled by policy", tool_id, risk, cap,
                           "P2_deny_list")

        # P3 — blocked subject
        if subject in self.deny_subjects:
            return self._d(Decision.DENY, "subject_blocked",
                           f"subject {subject!r} is blocked", tool_id, risk, cap,
                           "P3_subject_blocked")

        # P4 — capability gate
        if cap and cap not in caps and "*" not in caps:
            return self._d(Decision.DENY, "capabilitydenied",
                           f"session lacks capability {cap!r}", tool_id, risk, cap,
                           "P4_missing_capability")

        # P5 — private store paths
        path = str(args.get("path") or "")
        if not risk_only and tool.id.startswith("store.") and _under(path, "private"):
            if PRIVATE_CAP not in caps:
                return self._d(Decision.DENY, "private_store_denied",
                               "private/ requires the store:private capability",
                               tool_id, risk, cap, "P5_private_store")

        # P6 — write scope (mkdir may only create inside public/ or agents/)
        if not risk_only and tool.id == "store.mkdir":
            target = "/".join(filter(None, (str(args.get("parent") or ""),
                                            str(args.get("name") or ""))))
            if target.split("/", 1)[0] not in WRITE_ROOTS:
                return self._d(Decision.DENY, "write_scope_denied",
                               f"writes are restricted to {WRITE_ROOTS}", tool_id,
                               risk, cap, "P6_write_scope")

        # P7/P8/P9 — risk class
        if tool.risk is Risk.R2:
            return self._d(Decision.REQUIRE_CONFIRMATION, "requires_human_approval",
                           "privileged action requires a human approval token",
                           tool_id, risk, cap, "P7_r2_confirmation")
        if tool.risk is Risk.R1:
            return self._d(Decision.ALLOW, "allowed_r1",
                           "reversible write, audited", tool_id, risk, cap,
                           "P8_r1_allow")
        return self._d(Decision.ALLOW, "allowed", "read-only", tool_id, risk, cap,
                       "P9_r0_allow")

    def matrix(self, tools: Iterable[ToolSpec], subject: str,
               capabilities: Iterable[str]) -> list[dict[str, Any]]:
        return [self.evaluate(tool, subject, capabilities).as_dict() for tool in tools]

    def rules_snapshot(self) -> dict[str, Any]:
        return {"rules": [r.as_dict() for r in self.rules],
                "deny_tools": sorted(self.deny_tools),
                "deny_subjects": sorted(self.deny_subjects),
                "private_capability": PRIVATE_CAP,
                "write_roots": list(WRITE_ROOTS)}

    # ---------------------------------------------------------------- helper
    @staticmethod
    def _d(decision: Decision, code: str, reason: str, tool: str, risk: str,
           capability: str, rule: str) -> PolicyDecision:
        return PolicyDecision(decision=decision, code=code, reason=reason, tool=tool,
                              risk=risk, capability=capability, rule=rule)


def _under(path: str, root: str) -> bool:
    value = (path or "").strip().lstrip("/")
    return value == root or value.startswith(root + "/")

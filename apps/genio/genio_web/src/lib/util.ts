import type { Decision, Risk } from "./types";

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

export function uid(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function newSessionId(): string {
  return `ses_web_${uid()}`;
}

export function pct(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `${value.toFixed(1)}%`;
}

export function num(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : value.toLocaleString("en-US");
}

export function mb(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return value >= 1024 ? `${(value / 1024).toFixed(1)} GB` : `${value.toFixed(0)} MB`;
}

export function uptime(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "—";
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m ${Math.floor(seconds % 60)}s`;
}

export function clock(iso: string | number | undefined | null): string {
  if (iso === null || iso === undefined) return "--:--:--";
  const date = typeof iso === "number" ? new Date(iso) : new Date(iso);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return date.toLocaleTimeString("en-GB", { hour12: false });
}

export function shortHash(hash: string | undefined | null, head = 10): string {
  if (!hash) return "—";
  return hash.length <= head * 2 ? hash : `${hash.slice(0, head)}…${hash.slice(-6)}`;
}

export function clamp(value: number, min = 0, max = 100): number {
  return Math.min(max, Math.max(min, value));
}

export function riskClass(risk: Risk | string | undefined): string {
  switch (risk) {
    case "R0":
      return "border-mint/40 text-mint";
    case "R1":
      return "border-cyan/40 text-cyan";
    case "R2":
      return "border-amber/50 text-amber";
    default:
      return "border-line text-muted";
  }
}

export function decisionClass(decision: Decision | string | undefined): string {
  switch (decision) {
    case "ALLOW":
      return "border-mint/40 bg-mint/10 text-mint";
    case "DENY":
      return "border-rose/40 bg-rose/10 text-rose";
    case "REQUIRE_CONFIRMATION":
      return "border-amber/40 bg-amber/10 text-amber";
    default:
      return "border-line bg-panel2 text-muted";
  }
}

export function decisionLabel(decision: Decision | string | undefined): string {
  if (decision === "REQUIRE_CONFIRMATION") return "APPROVAL";
  return decision ?? "—";
}

export const RULE_TEXT: Record<string, string> = {
  P1_unknown_tool: "The tool id is not registered.",
  P2_deny_list: "The tool is on the hard deny-list — no capability can unlock it.",
  P3_subject_blocked: "This session identity is blocked by policy.",
  P4_missing_capability: "The signed session token does not carry this capability.",
  P5_private_store: "private/ requires the store:private capability.",
  P6_write_scope: "Writes are restricted to public/ and agents/.",
  P7_r2_confirmation: "Privileged R2 action — a human must approve this exact call.",
  P8_r1_allow: "Reversible write with mandatory audit.",
  P9_r0_allow: "Read-only action.",
};

export function ruleText(rule: string | undefined): string {
  if (!rule) return "No rule reported.";
  return RULE_TEXT[rule] ?? rule;
}

export function phaseLabel(phase: string | undefined): string {
  switch (phase) {
    case "perceive":
      return "PERCEIVE";
    case "plan":
      return "PLAN";
    case "policy":
      return "POLICY";
    case "act":
      return "ACT";
    case "observation":
      return "OBSERVE";
    case "reflect":
      return "REFLECT";
    case "final":
      return "FINAL";
    case "approval_required":
      return "APPROVAL";
    case "error":
      return "ERROR";
    default:
      return (phase ?? "event").toUpperCase();
  }
}

import { useEffect, useState } from "react";

import { useGenio } from "../state/GenioProvider";
import { cx, riskClass, ruleText } from "../lib/util";
import { IconAlert, IconCheck, IconLock, IconX } from "./icons";
import { Badge, Spinner } from "./ui";

function JsonBlock({ value }: { value: Record<string, unknown> | undefined }) {
  if (!value || Object.keys(value).length === 0) {
    return <p className="font-mono text-xs text-faint">no arguments</p>;
  }
  return (
    <pre className="mono max-h-44 overflow-auto rounded-lg border border-line/70 bg-void/70 p-3 text-[11px] leading-relaxed text-ink/90">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}

export function ApprovalModal() {
  const { approval, grant, deny, dismissApproval, deciding } = useGenio();
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    setSeconds(approval?.expires_in_s ?? 0);
    if (!approval) return;
    const timer = window.setInterval(() => setSeconds((prev) => Math.max(0, prev - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [approval]);

  useEffect(() => {
    if (!approval) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") dismissApproval();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [approval, dismissApproval]);

  if (!approval) return null;
  const rule = approval.policy?.rule;
  const expired = seconds <= 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-end justify-center bg-void/80 p-3 backdrop-blur-sm sm:items-center sm:p-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby="approval-title"
    >
      <div className="animate-rise w-full max-w-xl overflow-hidden rounded-xl border border-amber/30 bg-panel shadow-[0_24px_80px_-20px_rgba(0,0,0,0.9)]">
        <header className="flex items-start gap-3 border-b border-line/70 bg-amber/5 px-5 py-4">
          <span className="mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-md border border-amber/40 bg-amber/10 text-amber">
            <IconLock className="h-4 w-4" />
          </span>
          <div className="min-w-0 flex-1">
            <p className="label text-amber/80">human authorization required</p>
            <h2 id="approval-title" className="mt-1 font-mono text-sm text-ink">
              {approval.tool}
            </h2>
            <p className="mt-1 text-xs text-muted">
              Policy paused this exact call. Nothing has been executed.
            </p>
          </div>
          <button
            type="button"
            onClick={dismissApproval}
            className="text-muted transition hover:text-ink"
            aria-label="close approval dialog"
          >
            <IconX className="h-4 w-4" />
          </button>
        </header>

        <div className="space-y-4 px-5 py-4">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone="amber">{approval.risk ?? "R2"}</Badge>
            {approval.policy?.decision ? (
              <Badge tone="amber">{approval.policy.decision}</Badge>
            ) : null}
            {rule ? <Badge tone="line">{rule}</Badge> : null}
            <span
              className={cx(
                "ml-auto font-mono text-[11px]",
                expired ? "text-rose" : seconds <= 30 ? "text-amber" : "text-muted",
              )}
            >
              {expired ? "request expired" : `expires in ${seconds}s`}
            </span>
          </div>

          <div>
            <p className="label mb-2">arguments</p>
            <JsonBlock value={approval.args} />
            {!approval.args && approval.intent ? (
              <p className="mt-2 font-mono text-[11px] text-muted">intent: {approval.intent}</p>
            ) : null}
          </div>

          {approval.policy ? (
            <div className="rounded-lg border border-line/70 bg-panel2/60 p-3">
              <div className="flex items-center gap-2">
                <span className="label">policy verdict</span>
                <Badge className={riskClass(approval.policy.risk)}>{approval.policy.risk}</Badge>
              </div>
              <p className="mt-2 text-xs leading-relaxed text-ink/90">
                {ruleText(rule)}
              </p>
              <p className="mt-1 font-mono text-[11px] text-muted">
                {approval.policy.code} · {approval.policy.capability} · {approval.policy.reason}
              </p>
            </div>
          ) : null}

          <div className="flex items-start gap-2 rounded-lg border border-line/60 bg-void/40 p-3 text-[11px] leading-relaxed text-muted">
            <IconAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber/80" />
            <p>
              <span className="text-ink/90">Grant</span> issues a single-use, argument-bound
              token (120&nbsp;s). It cannot be reused for different arguments or replayed.
              Denial cancels the paused turn.
            </p>
          </div>
        </div>

        <footer className="flex flex-wrap items-center gap-2 border-t border-line/70 bg-panel2/50 px-5 py-3">
          <span className="mr-auto font-mono text-[10px] uppercase tracking-[0.16em] text-faint">
            {approval.approval_id}
          </span>
          <button type="button" className="btn btn-danger" onClick={() => void deny()} disabled={deciding}>
            <IconX className="h-3.5 w-3.5" />
            Deny
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void grant()}
            disabled={deciding || expired}
          >
            {deciding ? <Spinner className="h-3.5 w-3.5" /> : <IconCheck className="h-3.5 w-3.5" />}
            Grant &amp; continue
          </button>
        </footer>
      </div>
    </div>
  );
}

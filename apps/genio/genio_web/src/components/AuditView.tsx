import { useCallback, useEffect, useState } from "react";

import { api } from "../lib/api";
import type { AuditRecord, AuditVerify } from "../lib/types";
import { clock, cx, shortHash } from "../lib/util";
import { IconCheck, IconRefresh, IconScroll, IconX } from "./icons";
import { Badge, Empty, Panel, PanelHead, Spinner } from "./ui";

function decisionTone(decision: string): string {
  switch (decision) {
    case "executed":
      return "text-mint";
    case "denied":
    case "failed":
      return "text-rose";
    case "approval_requested":
    case "authorized":
      return "text-amber";
    default:
      return "text-muted";
  }
}

export function AuditView() {
  const [verify, setVerify] = useState<AuditVerify | null>(null);
  const [records, setRecords] = useState<AuditRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [chain, tail] = await Promise.all([api.auditVerify(), api.auditTail(60)]);
      setVerify(chain);
      setRecords([...tail.records].reverse());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="space-y-4">
      <Panel grid>
        <div className="flex flex-wrap items-center gap-4 px-5 py-4">
          <div className="flex items-center gap-3">
            <span
              className={cx(
                "grid h-10 w-10 place-items-center rounded-lg border",
                verify?.ok
                  ? "border-mint/30 bg-mint/5 text-mint"
                  : "border-rose/30 bg-rose/5 text-rose",
              )}
            >
              <IconScroll className="h-5 w-5" />
            </span>
            <div>
              <h2 className="font-mono text-sm tracking-[0.12em] text-ink">AUDIT CHAIN</h2>
              <p className="font-mono text-[10px] text-muted">
                SHA-256 linked · append-only · denies included
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            {verify ? (
              verify.ok ? (
                <Badge tone="mint">
                  <IconCheck className="h-3 w-3" /> chain valid
                </Badge>
              ) : (
                <Badge tone="rose">
                  <IconX className="h-3 w-3" /> broken chain
                </Badge>
              )
            ) : null}
            <Badge>{verify?.records ?? 0} records</Badge>
            <Badge tone="line">seq {verify?.seq ?? 0}</Badge>
          </div>
          <button type="button" onClick={() => void load()} className="btn btn-ghost ml-auto" disabled={loading}>
            {loading ? <Spinner className="h-3.5 w-3.5" /> : <IconRefresh className="h-3.5 w-3.5" />}
            verify
          </button>
        </div>
        <div className="grid gap-3 border-t border-line/60 px-5 py-3 font-mono text-[10px] sm:grid-cols-2">
          <div className="min-w-0">
            <p className="label">head</p>
            <p className="mt-1 truncate text-muted" title={verify?.head}>
              {shortHash(verify?.head, 18)}
            </p>
          </div>
          <div className="min-w-0">
            <p className="label">ledger</p>
            <p className="mt-1 truncate text-faint" title={verify?.path}>
              {verify?.path ?? "—"}
            </p>
          </div>
        </div>
        {error ? (
          <p className="border-t border-rose/20 bg-rose/5 px-5 py-2 font-mono text-[11px] text-rose">
            {error}
          </p>
        ) : null}
      </Panel>

      <Panel>
        <PanelHead
          title="recent decisions"
          hint="most recent first · every proposal leaves a record"
          right={<Badge>{records.length}</Badge>}
        />
        {records.length === 0 ? (
          <Empty title="ledger empty" hint="No tool calls have been audited yet." />
        ) : (
          <div className="scroll max-h-[560px] overflow-auto">
            <table className="w-full text-left font-mono text-[11px]">
              <thead className="sticky top-0 z-10 bg-panel/95 text-[9px] uppercase tracking-[0.12em] text-faint">
                <tr>
                  <th className="px-4 py-2 font-normal">seq</th>
                  <th className="px-2 py-2 font-normal">time</th>
                  <th className="px-2 py-2 font-normal">subject</th>
                  <th className="px-2 py-2 font-normal">tool</th>
                  <th className="px-2 py-2 font-normal">decision</th>
                  <th className="px-4 py-2 font-normal">detail</th>
                </tr>
              </thead>
              <tbody>
                {records.map((record) => (
                  <tr key={record.seq} className="border-t border-line/40 align-top">
                    <td className="px-4 py-2 text-faint">{record.seq}</td>
                    <td className="whitespace-nowrap px-2 py-2 text-muted">{clock(record.ts)}</td>
                    <td className="max-w-[110px] truncate px-2 py-2 text-muted">{record.actor}</td>
                    <td className="whitespace-nowrap px-2 py-2 text-ink/90">{record.tool}</td>
                    <td className={cx("whitespace-nowrap px-2 py-2", decisionTone(record.decision))}>
                      {record.decision}
                    </td>
                    <td className="max-w-[420px] px-4 py-2 text-[10px] leading-snug text-muted">
                      {record.detail}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="flex items-center gap-2 border-t border-line/60 px-4 py-2 font-mono text-[10px] text-faint">
          <span>GET /audit/verify</span>
          <span>·</span>
          <span>GET /audit/tail?n=60</span>
          <span className="ml-auto">hash {shortHash(verify?.head, 12)}</span>
        </div>
      </Panel>
    </div>
  );
}

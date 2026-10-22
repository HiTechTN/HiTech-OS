import { useMemo, useState } from "react";

import { useGenio } from "../state/GenioProvider";
import { cx, decisionClass, decisionLabel, riskClass } from "../lib/util";
import { IconRefresh, IconShield } from "./icons";
import { Badge, Empty, Panel, PanelHead, Spinner } from "./ui";

type Filter = "ALL" | "ALLOW" | "DENY" | "REQUIRE_CONFIRMATION";

export function PolicyView() {
  const { policy, refreshCatalogue, health } = useGenio();
  const [filter, setFilter] = useState<Filter>("ALL");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);

  const rows = useMemo(() => {
    const matrix = policy?.matrix ?? [];
    const needle = query.trim().toLowerCase();
    return matrix
      .filter((row) => (filter === "ALL" ? true : row.decision === filter))
      .filter((row) =>
        needle.length === 0
          ? true
          : `${row.tool} ${row.rule} ${row.code} ${row.reason}`.toLowerCase().includes(needle),
      )
      .sort((a, b) => a.tool.localeCompare(b.tool));
  }, [policy, filter, query]);

  const refresh = async () => {
    setLoading(true);
    try {
      await refreshCatalogue();
    } finally {
      setLoading(false);
    }
  };

  const counts = useMemo(() => {
    const matrix = policy?.matrix ?? [];
    return {
      total: matrix.length,
      allow: matrix.filter((row) => row.decision === "ALLOW").length,
      deny: matrix.filter((row) => row.decision === "DENY").length,
      confirm: matrix.filter((row) => row.decision === "REQUIRE_CONFIRMATION").length,
    };
  }, [policy]);

  if (!policy) {
    return (
      <Panel className="grid min-h-[280px] place-items-center">
        <Spinner className="h-5 w-5" />
      </Panel>
    );
  }

  return (
    <div className="space-y-4">
      <Panel grid>
        <div className="flex flex-wrap items-center gap-4 px-5 py-4">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-lg border border-amber/25 bg-amber/5 text-amber">
              <IconShield className="h-5 w-5" />
            </span>
            <div>
              <h2 className="font-mono text-sm tracking-[0.12em] text-ink">POLICY ENGINE</h2>
              <p className="font-mono text-[10px] text-muted">
                deterministic · ordered rules · no model opinion
              </p>
            </div>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <Badge tone="line">{policy.rules.length} rules</Badge>
            <span className="self-center px-1 font-mono text-[9px] uppercase tracking-[0.16em] text-faint">
              decisions
            </span>
            <Badge tone="mint">{counts.allow} allow</Badge>
            <Badge tone="amber">{counts.confirm} approval</Badge>
            <Badge tone="rose">{counts.deny} deny</Badge>
          </div>
          <button
            type="button"
            onClick={() => void refresh()}
            className="btn btn-ghost ml-auto"
            disabled={loading}
          >
            {loading ? <Spinner className="h-3.5 w-3.5" /> : <IconRefresh className="h-3.5 w-3.5" />}
            reload
          </button>
        </div>
        <div className="grid gap-3 border-t border-line/60 px-5 py-3 sm:grid-cols-3">
          <div>
            <p className="label">private capability</p>
            <p className="mono mt-1 text-[11px] text-amber">{policy.private_capability}</p>
          </div>
          <div>
            <p className="label">write roots</p>
            <p className="mono mt-1 text-[11px] text-ink">{policy.write_roots.join(" · ")}</p>
          </div>
          <div>
            <p className="label">deny list</p>
            <p className="mono mt-1 truncate text-[11px] text-rose" title={policy.deny_tools.join(", ")}>
              {policy.deny_tools.join(" · ")}
            </p>
          </div>
        </div>
      </Panel>

      <div className="grid gap-4 xl:grid-cols-[380px_minmax(0,1fr)]">
        <Panel>
          <PanelHead
            title="rules"
            hint="first match wins, ordered P1 → P9"
            right={<Badge>{policy.rules.length}</Badge>}
          />
          <ol className="divide-y divide-line/50">
            {policy.rules.map((rule) => (
              <li key={rule.id} className="px-4 py-3">
                <div className="flex items-center gap-2">
                  <span className="mono text-[10px] text-faint">{rule.id}</span>
                  <span className={cx("chip ml-auto", decisionClass(rule.decision))}>
                    {decisionLabel(rule.decision)}
                  </span>
                </div>
                <p className="mt-1 text-[11px] leading-relaxed text-muted">{rule.description}</p>
              </li>
            ))}
          </ol>
        </Panel>

        <Panel>
          <PanelHead
            title="decision matrix"
            hint="default agent session capabilities"
            right={
              <div className="flex items-center gap-1">
                {(["ALL", "ALLOW", "REQUIRE_CONFIRMATION", "DENY"] as Filter[]).map((item) => (
                  <button
                    key={item}
                    type="button"
                    onClick={() => setFilter(item)}
                    className={cx(
                      "rounded border px-2 py-1 font-mono text-[9px] uppercase tracking-[0.08em]",
                      filter === item
                        ? "border-cyan/40 bg-cyan/10 text-cyan"
                        : "border-line text-muted hover:text-ink",
                    )}
                  >
                    {item === "REQUIRE_CONFIRMATION" ? "approval" : item.toLowerCase()}
                  </button>
                ))}
              </div>
            }
          />
          <div className="border-b border-line/60 px-4 py-2">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="filter tools, rules, codes…"
              className="w-full rounded-md border border-line bg-void/60 px-2.5 py-1.5 font-mono text-[11px] text-ink outline-none placeholder:text-faint focus:border-cyan/40"
            />
          </div>
          <div className="scroll max-h-[520px] overflow-auto">
            {rows.length === 0 ? (
              <Empty title="no rows match" hint="Adjust the filter or query." />
            ) : (
              <table className="w-full text-left">
                <thead className="sticky top-0 z-10 bg-panel/95 font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
                  <tr>
                    <th className="px-4 py-2 font-normal">tool</th>
                    <th className="px-2 py-2 font-normal">risk</th>
                    <th className="px-2 py-2 font-normal">decision</th>
                    <th className="px-2 py-2 font-normal">rule / code</th>
                    <th className="px-4 py-2 font-normal">reason</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.tool} className="border-t border-line/40 align-top">
                      <td className="whitespace-nowrap px-4 py-2 font-mono text-[11px] text-ink">
                        {row.tool}
                      </td>
                      <td className="px-2 py-2">
                        <span className={cx("chip", riskClass(row.risk))}>{row.risk}</span>
                      </td>
                      <td className="px-2 py-2">
                        <span className={cx("chip", decisionClass(row.decision))}>
                          {decisionLabel(row.decision)}
                        </span>
                      </td>
                      <td className="px-2 py-2 font-mono text-[10px] text-muted">
                        {row.rule}
                        <br />
                        <span className="text-faint">{row.code}</span>
                      </td>
                      <td className="px-4 py-2 text-[10px] leading-snug text-muted">{row.reason}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t border-line/60 px-4 py-2 font-mono text-[10px] text-faint">
            <span>{rows.length} rows</span>
            <span>·</span>
            <span>session caps: {health?.registry.capabilities.length ?? "—"} total</span>
            <span className="ml-auto">GET /policy</span>
          </div>
        </Panel>
      </div>
    </div>
  );
}

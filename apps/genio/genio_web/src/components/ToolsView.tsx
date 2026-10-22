import { useMemo, useState } from "react";

import { useGenio } from "../state/GenioProvider";
import type { Risk, ToolInfo } from "../lib/types";
import { cx, riskClass } from "../lib/util";
import { IconLock, IconRefresh, IconWrench } from "./icons";
import { Badge, Empty, Panel, PanelHead, Spinner } from "./ui";

function ToolCard({ tool }: { tool: ToolInfo }) {
  return (
    <article className="flex flex-col gap-3 rounded-xl border border-line/70 bg-panel2/40 p-4 transition hover:border-cyan/25">
      <header className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <h3 className="truncate font-mono text-[12px] text-ink">{tool.id}</h3>
          <p className="mt-0.5 truncate font-mono text-[10px] text-cyan/70">{tool.cap}</p>
        </div>
        <span className={cx("chip shrink-0", riskClass(tool.risk))}>{tool.risk}</span>
      </header>

      <p className="min-h-[32px] text-[11px] leading-relaxed text-muted">{tool.description}</p>

      {Object.keys(tool.params).length > 0 ? (
        <div className="space-y-1">
          {Object.entries(tool.params).map(([name, param]) => (
            <p key={name} className="flex items-center gap-2 font-mono text-[10px]">
              <span className="text-ink/90">{name}</span>
              <span className="text-faint">{param.type}</span>
              {param.required ? <span className="text-rose/80">required</span> : null}
            </p>
          ))}
        </div>
      ) : (
        <p className="font-mono text-[10px] text-faint">no arguments</p>
      )}

      <footer className="mt-auto flex flex-wrap items-center gap-1.5 border-t border-line/50 pt-2.5">
        {tool.tags.map((tag) => (
          <span key={tag} className="chip text-faint">
            {tag}
          </span>
        ))}
        <span className="ml-auto font-mono text-[10px] text-faint">{tool.rate_per_min}/min</span>
        {tool.risk === "R2" ? (
          <span className="flex items-center gap-1 font-mono text-[10px] text-amber">
            <IconLock className="h-3 w-3" /> approval
          </span>
        ) : null}
      </footer>
    </article>
  );
}

export function ToolsView() {
  const { tools, refreshCatalogue, health } = useGenio();
  const [query, setQuery] = useState("");
  const [risk, setRisk] = useState<Risk | "ALL">("ALL");
  const [loading, setLoading] = useState(false);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return tools
      .filter((tool) => (risk === "ALL" ? true : tool.risk === risk))
      .filter((tool) =>
        needle.length === 0
          ? true
          : `${tool.id} ${tool.description} ${tool.cap} ${tool.tags.join(" ")}`
              .toLowerCase()
              .includes(needle),
      )
      .sort((a, b) => a.id.localeCompare(b.id));
  }, [tools, query, risk]);

  const refresh = async () => {
    setLoading(true);
    try {
      await refreshCatalogue();
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <Panel grid>
        <div className="flex flex-wrap items-center gap-4 px-5 py-4">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-lg border border-cyan/25 bg-cyan/5 text-cyan">
              <IconWrench className="h-5 w-5" />
            </span>
            <div>
              <h2 className="font-mono text-sm tracking-[0.12em] text-ink">TOOL REGISTRY</h2>
              <p className="font-mono text-[10px] text-muted">
                capability per tool · validators run before risk
              </p>
            </div>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <Badge tone="line">{tools.length} tools</Badge>
            <Badge tone="mint">{tools.filter((tool) => tool.risk === "R0").length} R0</Badge>
            <Badge tone="cyan">{tools.filter((tool) => tool.risk === "R1").length} R1</Badge>
            <Badge tone="amber">{tools.filter((tool) => tool.risk === "R2").length} R2</Badge>
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
        <div className="flex flex-wrap items-center gap-2 border-t border-line/60 px-5 py-3">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="search id, capability, tag…"
            className="min-w-[220px] flex-1 rounded-md border border-line bg-void/60 px-2.5 py-1.5 font-mono text-[11px] text-ink outline-none placeholder:text-faint focus:border-cyan/40"
          />
          <div className="flex gap-1">
            {(["ALL", "R0", "R1", "R2"] as const).map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => setRisk(item)}
                className={cx(
                  "rounded border px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.1em]",
                  risk === item
                    ? "border-cyan/40 bg-cyan/10 text-cyan"
                    : "border-line text-muted hover:text-ink",
                )}
              >
                {item.toLowerCase()}
              </button>
            ))}
          </div>
        </div>
      </Panel>

      {filtered.length === 0 ? (
        <Panel>
          <Empty title="no tools match" hint="Adjust the search or risk filter." />
        </Panel>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {filtered.map((tool) => (
            <ToolCard key={tool.id} tool={tool} />
          ))}
        </div>
      )}

      <Panel>
        <PanelHead
          title="capabilities in effect"
          hint="registry-wide union, granted per session by the daemon"
          right={<Badge>{health?.registry.capabilities.length ?? "—"}</Badge>}
        />
        <div className="flex flex-wrap gap-1.5 px-4 py-3">
          {(health?.registry.capabilities ?? []).map((cap) => (
            <span key={cap} className="chip border-cyan/20 text-cyan/80">
              {cap}
            </span>
          ))}
        </div>
      </Panel>
    </div>
  );
}

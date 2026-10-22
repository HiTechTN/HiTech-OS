import type { ReactNode } from "react";

import type { GenioStatus, Health } from "../lib/types";
import { cx, num, uptime } from "../lib/util";
import {
  IconActivity,
  IconChevron,
  IconHex,
  IconScroll,
  IconShield,
  IconTerminal,
  IconWrench,
} from "./icons";
import { Badge, StatusPill } from "./ui";

export type TabId = "console" | "telemetry" | "policy" | "tools" | "audit";

export const TABS: Array<{
  id: TabId;
  label: string;
  caption: string;
  icon: (props: { className?: string }) => ReactNode;
}> = [
  { id: "console", label: "Console", caption: "agent + approvals", icon: IconTerminal },
  { id: "telemetry", label: "Telemetry", caption: "cpu · ram · services", icon: IconActivity },
  { id: "policy", label: "Policy", caption: "rules + matrix", icon: IconShield },
  { id: "tools", label: "Tools", caption: "registry", icon: IconWrench },
  { id: "audit", label: "Audit", caption: "hash chain", icon: IconScroll },
];

function Dot() {
  return <span className="h-1 w-1 rounded-full bg-faint" />;
}

export function Shell({
  tab,
  onTab,
  health,
  status,
  connected,
  pendingCount,
  children,
}: {
  tab: TabId;
  onTab: (tab: TabId) => void;
  health: Health | null;
  status: GenioStatus;
  connected: boolean;
  pendingCount: number;
  children: ReactNode;
}) {
  const version = health?.version ?? "connecting…";
  return (
    <div className="min-h-full">
      <header className="sticky top-0 z-30 border-b border-line/70 bg-void/85 backdrop-blur-md">
        <div className="mx-auto flex max-w-[1500px] items-center gap-4 px-4 py-3 sm:px-6">
          <div className="flex min-w-0 items-center gap-3">
            <span className="relative grid h-9 w-9 place-items-center rounded-lg border border-cyan/30 bg-cyan/5 text-cyan">
              <IconHex className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <div className="flex items-baseline gap-2">
                <span className="font-mono text-sm font-semibold tracking-[0.28em] text-ink">
                  GENIO
                </span>
                <span className="hidden font-mono text-[10px] uppercase tracking-[0.2em] text-muted sm:inline">
                  chronos portal
                </span>
              </div>
              <p className="truncate font-mono text-[10px] text-faint">
                sovereign agent control plane
              </p>
            </div>
          </div>

          <div className="hidden flex-1 items-center justify-center gap-3 lg:flex">
            <span className="label">HiTech-OS</span>
            <Dot />
            <span className="mono text-[11px] text-muted">{version}</span>
            <Dot />
            <span className="mono text-[11px] text-faint">
              up {uptime(health?.uptime_s)} · {num(health?.agent.ticks ?? 0)} ticks
            </span>
          </div>

          <div className="ml-auto flex items-center gap-2 sm:gap-3">
            <StatusPill status={status} compact />
            <span
              className={cx(
                "hidden items-center gap-1.5 rounded-full border px-2 py-1 font-mono text-[10px] uppercase tracking-[0.16em] sm:inline-flex",
                connected
                  ? "border-mint/30 bg-mint/5 text-mint"
                  : "border-rose/30 bg-rose/5 text-rose",
              )}
              title="Server-Sent Events stream"
            >
              <span
                className={cx(
                  "h-1.5 w-1.5 rounded-full",
                  connected ? "bg-mint" : "bg-rose",
                )}
              />
              {connected ? "sse live" : "sse offline"}
            </span>
            {pendingCount > 0 ? (
              <Badge tone="amber">{pendingCount} pending</Badge>
            ) : null}
          </div>
        </div>

        <nav className="mx-auto flex max-w-[1500px] gap-1 overflow-x-auto px-3 pb-2 lg:hidden">
          {TABS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onTab(item.id)}
              className={cx(
                "flex shrink-0 items-center gap-2 rounded-md px-3 py-1.5 font-mono text-[10px] uppercase tracking-[0.14em]",
                tab === item.id
                  ? "bg-panel2 text-cyan"
                  : "text-muted hover:bg-panel2/60 hover:text-ink",
              )}
            >
              <item.icon className="h-3.5 w-3.5" />
              {item.label}
            </button>
          ))}
        </nav>
      </header>

      <div className="mx-auto flex max-w-[1500px] gap-5 px-4 py-5 sm:px-6">
        <aside className="hidden w-48 shrink-0 lg:block">
          <div className="sticky top-24 space-y-1">
            {TABS.map((item) => {
              const active = tab === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => onTab(item.id)}
                  className={cx("tab", active && "tab-active")}
                  aria-current={active ? "page" : undefined}
                >
                  <item.icon className={cx("h-4 w-4", active ? "text-cyan" : "text-faint")} />
                  <span className="flex-1 text-left">
                    <span className="block">{item.label}</span>
                    <span className="mt-0.5 block font-sans text-[10px] normal-case tracking-normal text-faint">
                      {item.caption}
                    </span>
                  </span>
                  {item.id === "console" && pendingCount > 0 ? (
                    <span className="rounded-full bg-amber/15 px-1.5 py-0.5 font-mono text-[10px] text-amber">
                      {pendingCount}
                    </span>
                  ) : (
                    <IconChevron
                      className={cx("h-3 w-3", active ? "text-cyan/70" : "text-faint/30")}
                    />
                  )}
                </button>
              );
            })}

            <div className="mt-6 rounded-lg border border-line/60 bg-panel/50 p-3">
              <p className="label">session</p>
              <p className="mt-2 font-mono text-[10px] leading-relaxed text-muted">
                loopback · signed
                <br />
                caps scoped per tool
              </p>
              <p className="mt-2 font-mono text-[10px] text-faint">
                audit seq {health?.broker.audit_seq ?? 0}
              </p>
            </div>
          </div>
        </aside>

        <main className="min-w-0 flex-1">{children}</main>
      </div>

      <footer className="mx-auto flex max-w-[1500px] flex-wrap items-center gap-x-3 gap-y-1 px-4 pb-6 font-mono text-[10px] text-faint sm:px-6">
        <span>genio {version}</span>
        <Dot />
        <span>tools {health?.registry.count ?? "—"}</span>
        <Dot />
        <span>policy rules {health?.policy.rules ?? "—"}</span>
        <Dot />
        <span>portal {health?.portal.auth_mode ?? "—"}</span>
        <span className="ml-auto">HiTechTN · sovereign by construction</span>
      </footer>
    </div>
  );
}

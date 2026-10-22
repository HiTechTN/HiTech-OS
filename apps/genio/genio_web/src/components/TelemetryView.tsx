import { useMemo, useState } from "react";

import { useGenio } from "../state/GenioProvider";
import { clock, cx, mb, num, pct, uptime } from "../lib/util";
import { IconActivity, IconAlert, IconCpu, IconMemory, IconRefresh, IconServer } from "./icons";
import { Badge, Empty, Gauge, Panel, PanelHead, Sparkline, Spinner } from "./ui";

function SparkCard({
  label,
  values,
  color,
  current,
}: {
  label: string;
  values: number[];
  color: string;
  current: string;
}) {
  const min = values.length ? Math.min(...values) : 0;
  const max = values.length ? Math.max(...values) : 0;
  return (
    <Panel className="p-4">
      <div className="flex items-baseline justify-between">
        <p className="label">{label}</p>
        <p className="mono text-sm text-ink">{current}</p>
      </div>
      <Sparkline values={values} stroke={color} />
      <div className="flex justify-between font-mono text-[10px] text-faint">
        <span>min {min.toFixed(0)}%</span>
        <span>max {max.toFixed(0)}%</span>
        <span>{values.length} samples</span>
      </div>
    </Panel>
  );
}

export function TelemetryView() {
  const { telemetry, history, refreshTelemetry } = useGenio();
  const [loading, setLoading] = useState(false);

  const cpuValues = useMemo(
    () => history.map((sample) => sample.cpu_percent ?? 0),
    [history],
  );
  const memValues = useMemo(
    () => history.map((sample) => sample.mem_percent ?? 0),
    [history],
  );

  const refresh = async () => {
    setLoading(true);
    try {
      await refreshTelemetry(true);
    } finally {
      setLoading(false);
    }
  };

  if (!telemetry) {
    return (
      <Panel className="grid min-h-[320px] place-items-center">
        <div className="flex flex-col items-center gap-2 text-muted">
          <Spinner className="h-5 w-5" />
          <p className="font-mono text-[11px] uppercase tracking-[0.18em]">
            waiting for telemetry
          </p>
        </div>
      </Panel>
    );
  }

  const cpu = telemetry.cpu;
  const memory = telemetry.memory;
  const services = telemetry.services;
  const failed = services?.failed_count ?? 0;

  return (
    <div className="space-y-4">
      <Panel grid>
        <div className="flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-lg border border-cyan/25 bg-cyan/5 text-cyan">
              <IconServer className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <h2 className="truncate font-mono text-sm tracking-[0.12em] text-ink">
                {telemetry.host.hostname}
              </h2>
              <p className="truncate font-mono text-[10px] text-muted">
                {telemetry.host.machine} · kernel {telemetry.host.kernel}
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone="mint">up {uptime(telemetry.uptime_s)}</Badge>
            <Badge>load {telemetry.load ? telemetry.load["1m"].toFixed(2) : "—"}</Badge>
            {telemetry.host.nixos_generation ? (
              <Badge tone="cyan">{telemetry.host.nixos_generation}</Badge>
            ) : (
              <Badge>non-nixos host</Badge>
            )}
            <Badge tone="line">collector {telemetry.collector_ms.toFixed(1)} ms</Badge>
          </div>
          <button
            type="button"
            onClick={() => void refresh()}
            className="btn btn-ghost ml-auto"
            disabled={loading}
          >
            {loading ? <Spinner className="h-3.5 w-3.5" /> : <IconRefresh className="h-3.5 w-3.5" />}
            refresh
          </button>
        </div>
        {telemetry.error ? (
          <p className="border-t border-rose/20 bg-rose/5 px-5 py-2 font-mono text-[11px] text-rose">
            collector error: {telemetry.error}
          </p>
        ) : null}
      </Panel>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <Panel className="p-4">
          <Gauge
            label="cpu load"
            value={cpu?.percent ?? 0}
            hint={`${num(cpu?.count)} logical · ${num(cpu?.physical)} physical${cpu?.freq_mhz ? ` · ${cpu.freq_mhz.toFixed(0)} MHz` : ""}`}
          />
          <div className="mt-3 flex flex-wrap gap-1">
            {(cpu?.per_core ?? []).slice(0, 12).map((value, index) => (
              <span
                key={index}
                title={`core ${index}: ${value}%`}
                className={cx(
                  "h-1.5 flex-1 rounded-full",
                  value > 85 ? "bg-rose" : value > 65 ? "bg-amber" : "bg-cyan/60",
                )}
                style={{ opacity: 0.35 + Math.min(0.65, value / 100) }}
              />
            ))}
          </div>
        </Panel>

        <Panel className="p-4">
          <Gauge
            label="memory"
            value={memory?.percent ?? 0}
            hint={`${mb(memory?.used_mb)} used of ${mb(memory?.total_mb)}`}
          />
          <div className="mt-3 space-y-1 font-mono text-[10px] text-muted">
            <p className="flex justify-between">
              <span>available</span>
              <span>{mb(memory?.available_mb)}</span>
            </p>
            <p className="flex justify-between">
              <span>swap</span>
              <span>
                {mb(memory?.swap_used_mb)} / {mb(memory?.swap_total_mb)}
              </span>
            </p>
          </div>
        </Panel>

        <Panel className="p-4">
          <p className="label">services</p>
          <div className="mt-2 flex items-baseline gap-2">
            <span className="mono text-3xl text-ink">{num(services?.running_count)}</span>
            <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted">
              running
            </span>
          </div>
          <div className="mt-3 flex items-center gap-2">
            {failed > 0 ? (
              <Badge tone="rose">
                <IconAlert className="h-3 w-3" /> {failed} failed
              </Badge>
            ) : (
              <Badge tone="mint">all healthy</Badge>
            )}
          </div>
          <p className="mt-3 line-clamp-2 font-mono text-[10px] leading-relaxed text-faint">
            {services?.sample?.slice(0, 4).join(" · ") ?? services?.error ?? "systemd unavailable"}
          </p>
        </Panel>

        <Panel className="p-4">
          <p className="label">processes</p>
          <div className="mt-2 flex items-baseline gap-2">
            <span className="mono text-3xl text-ink">{num(telemetry.processes?.count)}</span>
            <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted">
              observed
            </span>
          </div>
          <p className="mt-3 font-mono text-[10px] text-faint">
            top: {telemetry.processes?.top_by_rss?.[0]?.name ?? "—"}
          </p>
          <p className="mt-1 font-mono text-[10px] text-faint">
            {mb(telemetry.processes?.top_by_rss?.[0]?.rss_mb)}
          </p>
        </Panel>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <SparkCard
          label="cpu history"
          values={cpuValues}
          color="#22d3ee"
          current={pct(cpu?.percent)}
        />
        <SparkCard
          label="memory history"
          values={memValues}
          color="#34d399"
          current={pct(memory?.percent)}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <Panel>
          <PanelHead title="disks" hint="read-only mounts" right={<Badge tone="cyan">/ · /data · /nix</Badge>} />
          <div className="space-y-3 px-4 py-4">
            {Object.entries(telemetry.disks ?? {}).length === 0 ? (
              <Empty title="no mounts reported" />
            ) : (
              Object.entries(telemetry.disks ?? {}).map(([mount, disk]) => (
                <div key={mount}>
                  <div className="mb-1 flex items-center gap-2 font-mono text-[11px]">
                    <span className="text-ink">{mount}</span>
                    <span className="ml-auto text-muted">
                      {disk.used_gb.toFixed(1)} / {disk.total_gb.toFixed(1)} GB
                    </span>
                    <span className={cx((disk.percent ?? 0) > 85 ? "text-rose" : "text-cyan")}>
                      {pct(disk.percent)}
                    </span>
                  </div>
                  <div className="h-1.5 overflow-hidden rounded-full bg-panel2">
                    <div
                      className={cx(
                        "h-full",
                        (disk.percent ?? 0) > 85
                          ? "bg-rose"
                          : (disk.percent ?? 0) > 70
                            ? "bg-amber"
                            : "bg-cyan/70",
                      )}
                      style={{ width: `${Math.min(100, disk.percent ?? 0)}%` }}
                    />
                  </div>
                </div>
              ))
            )}
            {telemetry.sensors ? (
              <div className="flex flex-wrap gap-1.5 border-t border-line/60 pt-3">
                {Object.entries(telemetry.sensors).map(([sensor, value]) => (
                  <span key={sensor} className="chip">
                    {sensor} <span className="text-cyan">{value.toFixed(1)}°C</span>
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        </Panel>

        <Panel>
          <PanelHead
            title="top processes"
            hint="by resident memory"
            right={<Badge>{telemetry.processes?.top_by_rss?.length ?? 0}</Badge>}
          />
          <div className="scroll max-h-64 overflow-auto">
            <table className="w-full text-left font-mono text-[11px]">
              <thead className="sticky top-0 bg-panel/95 text-[10px] uppercase tracking-[0.12em] text-faint">
                <tr>
                  <th className="px-4 py-2 font-normal">pid</th>
                  <th className="px-2 py-2 font-normal">name</th>
                  <th className="px-2 py-2 text-right font-normal">rss</th>
                  <th className="px-4 py-2 text-right font-normal">cpu</th>
                </tr>
              </thead>
              <tbody>
                {(telemetry.processes?.top_by_rss ?? []).map((proc) => (
                  <tr key={`${proc.pid}-${proc.name}`} className="border-t border-line/40">
                    <td className="px-4 py-1.5 text-faint">{proc.pid}</td>
                    <td className="max-w-[160px] truncate px-2 py-1.5 text-ink/90">{proc.name}</td>
                    <td className="px-2 py-1.5 text-right text-muted">{mb(proc.rss_mb)}</td>
                    <td className="px-4 py-1.5 text-right text-muted">{pct(proc.cpu_percent)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      </div>

      {failed > 0 ? (
        <Panel className="border-rose/30">
          <PanelHead
            title="failed units"
            right={<Badge tone="rose">{failed}</Badge>}
            hint="systemd reports these as failed"
          />
          <ul className="divide-y divide-line/50">
            {(services?.failed_units ?? []).map((unit) => (
              <li key={unit} className="flex items-center gap-2 px-4 py-2 font-mono text-[11px] text-rose/90">
                <IconAlert className="h-3.5 w-3.5" />
                {unit}
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}

      <div className="flex flex-wrap items-center gap-2 font-mono text-[10px] text-faint">
        <IconCpu className="h-3 w-3" /> cpu
        <IconMemory className="h-3 w-3" /> memory
        <IconActivity className="h-3 w-3" /> sampled {clock(telemetry.timestamp)} · schema{" "}
        {telemetry.schema}
      </div>
    </div>
  );
}

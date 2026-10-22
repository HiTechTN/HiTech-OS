import type { ReactNode } from "react";

import type { GenioStatus } from "../lib/types";
import { clamp, cx } from "../lib/util";
import { IconAlert, IconX } from "./icons";

export function Panel({
  children,
  className,
  grid = false,
}: {
  children: ReactNode;
  className?: string;
  grid?: boolean;
}) {
  return (
    <section className={cx("panel overflow-hidden", grid && "grid-bg", className)}>
      {children}
    </section>
  );
}

export function PanelHead({
  title,
  hint,
  right,
}: {
  title: ReactNode;
  hint?: ReactNode;
  right?: ReactNode;
}) {
  return (
    <header className="panel-head">
      <div className="min-w-0">
        <h2 className="label text-ink/80">{title}</h2>
        {hint ? <p className="mt-1 truncate text-xs text-muted">{hint}</p> : null}
      </div>
      {right ? <div className="flex shrink-0 items-center gap-2">{right}</div> : null}
    </header>
  );
}

export function Badge({
  children,
  tone = "line",
  className,
}: {
  children: ReactNode;
  tone?: "line" | "mint" | "cyan" | "amber" | "rose" | "muted";
  className?: string;
}) {
  const tones: Record<string, string> = {
    line: "border-line text-muted",
    muted: "border-line text-faint",
    mint: "border-mint/40 bg-mint/10 text-mint",
    cyan: "border-cyan/40 bg-cyan/10 text-cyan",
    amber: "border-amber/40 bg-amber/10 text-amber",
    rose: "border-rose/40 bg-rose/10 text-rose",
  };
  return <span className={cx("chip", tones[tone], className)}>{children}</span>;
}

const STATUS_META: Record<
  GenioStatus,
  { label: string; dot: string; text: string; ring: string; detail: string }
> = {
  idle: {
    label: "Idle",
    dot: "bg-mint",
    text: "text-mint",
    ring: "border-mint/30 bg-mint/5",
    detail: "loop steady",
  },
  thinking: {
    label: "Thinking",
    dot: "bg-cyan pulse-ring",
    text: "text-cyan",
    ring: "border-cyan/30 bg-cyan/5",
    detail: "ReAct cycle running",
  },
  awaiting_approval: {
    label: "Awaiting approval",
    dot: "bg-amber pulse-ring",
    text: "text-amber",
    ring: "border-amber/40 bg-amber/10",
    detail: "privileged action paused",
  },
  degraded: {
    label: "Degraded",
    dot: "bg-rose",
    text: "text-rose",
    ring: "border-rose/30 bg-rose/5",
    detail: "degraded or offline",
  },
};

export function StatusPill({ status, compact = false }: { status: GenioStatus; compact?: boolean }) {
  const meta = STATUS_META[status];
  return (
    <span
      className={cx(
        "inline-flex items-center gap-2 rounded-full border px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.18em]",
        meta.ring,
        meta.text,
      )}
      title={meta.detail}
    >
      <span className={cx("h-1.5 w-1.5 rounded-full", meta.dot)} />
      {meta.label}
      {!compact ? <span className="text-faint">· {meta.detail}</span> : null}
    </span>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <svg
      className={cx("h-4 w-4 animate-spin text-cyan", className)}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.2" strokeWidth="2" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function Gauge({
  label,
  value,
  unit = "%",
  hint,
  invert = false,
}: {
  label: string;
  value: number | null | undefined;
  unit?: string;
  hint?: string;
  invert?: boolean;
}) {
  const ratio = clamp(value ?? 0) / 100;
  const color = invert
    ? "text-mint"
    : ratio >= 0.9
      ? "text-rose"
      : ratio >= 0.7
        ? "text-amber"
        : "text-cyan";
  const circumference = 2 * Math.PI * 34;
  return (
    <div className="flex items-center gap-4">
      <div className="relative h-20 w-20 shrink-0">
        <svg viewBox="0 0 80 80" className="h-full w-full -rotate-90">
          <circle cx="40" cy="40" r="34" fill="none" stroke="#16202b" strokeWidth="6" />
          <circle
            cx="40"
            cy="40"
            r="34"
            fill="none"
            stroke="currentColor"
            strokeWidth="6"
            strokeLinecap="round"
            strokeDasharray={`${Math.max(0.01, ratio) * circumference} ${circumference}`}
            className={cx("transition-all duration-700", color)}
          />
        </svg>
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className={cx("mono text-sm font-medium", color)}>
            {value === null || value === undefined ? "—" : `${value.toFixed(0)}${unit}`}
          </span>
        </div>
      </div>
      <div className="min-w-0">
        <p className="label">{label}</p>
        {hint ? <p className="mt-1 text-xs text-muted">{hint}</p> : null}
      </div>
    </div>
  );
}

export function Sparkline({
  values,
  className,
  stroke = "#22d3ee",
  height = 48,
}: {
  values: number[];
  className?: string;
  stroke?: string;
  height?: number;
}) {
  const width = 240;
  const data = values.length > 1 ? values : [0, ...values];
  const max = Math.max(100, ...data);
  const step = width / Math.max(1, data.length - 1);
  const points = data
    .map((value, index) => {
      const x = index * step;
      const y = height - 4 - (clamp(value) / max) * (height - 8);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const area = `0,${height} ${points} ${width},${height}`;
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className={cx("h-12 w-full", className)}
      aria-hidden
    >
      <defs>
        <linearGradient id="spark" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity="0.25" />
          <stop offset="100%" stopColor={stroke} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill="url(#spark)" />
      <polyline
        points={points}
        fill="none"
        stroke={stroke}
        strokeWidth="1.6"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

export function Empty({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="flex h-full min-h-32 flex-col items-center justify-center gap-1 px-4 py-8 text-center">
      <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-faint">{title}</p>
      {hint ? <p className="max-w-sm text-xs text-muted">{hint}</p> : null}
    </div>
  );
}

export function ErrorBanner({ message, onClose }: { message: string; onClose: () => void }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-rose/40 bg-rose/10 px-3 py-2 text-sm text-rose">
      <IconAlert className="mt-0.5 h-4 w-4 shrink-0" />
      <p className="min-w-0 flex-1 break-words font-mono text-xs">{message}</p>
      <button type="button" onClick={onClose} className="text-rose/70 hover:text-rose" aria-label="dismiss">
        <IconX className="h-4 w-4" />
      </button>
    </div>
  );
}

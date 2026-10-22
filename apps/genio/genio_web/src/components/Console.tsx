import { useEffect, useMemo, useRef, useState } from "react";

import type { AgentStep, ChatMessage, GenioEvent } from "../lib/types";
import { useGenio } from "../state/GenioProvider";
import { clock, cx, decisionClass, decisionLabel, phaseLabel, pct } from "../lib/util";
import {
  IconActivity,
  IconAlert,
  IconCheck,
  IconChevron,
  IconSend,
  IconShield,
  IconTerminal,
} from "./icons";
import { Badge, Empty, Panel, PanelHead, Spinner } from "./ui";

const SUGGESTIONS = [
  "How is the machine doing?",
  "Quanti file ci sono nella cartella public?",
  "Riassumi public/LANCIO UNIEMENS.docx",
  "Crea la cartella public/genio-demo",
];

function StepRow({ step }: { step: AgentStep }) {
  return (
    <div className="border-t border-line/50 px-3 py-2 first:border-t-0">
      <div className="flex flex-wrap items-center gap-2">
        <span className="mono text-[10px] text-faint">#{step.step}</span>
        <span className="font-mono text-[11px] text-ink">{step.tool ?? "model"}</span>
        {step.decision ? (
          <span className={cx("chip", decisionClass(step.decision))}>{step.decision}</span>
        ) : null}
        {step.audit_seq ? (
          <span className="ml-auto font-mono text-[10px] text-faint">audit #{step.audit_seq}</span>
        ) : null}
      </div>
      {step.args && Object.keys(step.args).length > 0 ? (
        <pre className="mono mt-1 overflow-x-auto text-[10px] text-muted">
          {JSON.stringify(step.args)}
        </pre>
      ) : null}
      <p className="mt-1 whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed text-muted">
        {step.observation}
      </p>
    </div>
  );
}

function Message({ message }: { message: ChatMessage }) {
  const [open, setOpen] = useState(false);
  const isUser = message.role === "user";
  const pending = message.status === "awaiting_approval";
  const failed = message.status === "error";
  const denied = message.status === "denied";

  return (
    <article
      className={cx(
        "animate-rise px-4 py-3.5 sm:px-5",
        isUser ? "border-l-2 border-cyan/50 bg-cyan/[0.025]" : "border-l-2 border-line",
      )}
    >
      <header className="mb-1.5 flex flex-wrap items-center gap-2">
        <span
          className={cx(
            "font-mono text-[10px] uppercase tracking-[0.18em]",
            isUser ? "text-cyan/80" : "text-mint/80",
          )}
        >
          {isUser ? "operator" : "genio"}
        </span>
        <span className="font-mono text-[10px] text-faint">{clock(message.ts)}</span>
        {pending ? <Badge tone="amber">awaiting approval</Badge> : null}
        {failed ? <Badge tone="rose">error</Badge> : null}
        {denied ? <Badge tone="muted">denied</Badge> : null}
        {message.degraded ? <Badge tone="amber">degraded</Badge> : null}
        {message.status === "thinking" ? (
          <span className="ml-auto flex items-center gap-1.5 font-mono text-[10px] text-cyan">
            <Spinner className="h-3 w-3" /> thinking
          </span>
        ) : null}
      </header>

      {message.text ? (
        <p
          className={cx(
            "whitespace-pre-wrap break-words text-[13px] leading-relaxed",
            failed ? "text-rose/90" : denied ? "text-muted" : "text-ink/90",
            message.status === "thinking" && "caret text-muted",
          )}
        >
          {message.text}
        </p>
      ) : null}

      {pending ? (
        <p className="mt-2 rounded-md border border-amber/30 bg-amber/5 px-3 py-2 font-mono text-[11px] text-amber">
          privileged action paused — decide in the authorization dialog
        </p>
      ) : null}

      {message.error ? (
        <p className="mt-2 font-mono text-[11px] text-rose">{message.error}</p>
      ) : null}

      {message.steps && message.steps.length > 0 ? (
        <div className="mt-2 overflow-hidden rounded-lg border border-line/60 bg-void/30">
          <button
            type="button"
            onClick={() => setOpen((value) => !value)}
            className="flex w-full items-center gap-2 px-3 py-2 text-left font-mono text-[10px] uppercase tracking-[0.14em] text-muted transition hover:text-ink"
          >
            <IconChevron className={cx("h-3 w-3 transition", open && "rotate-90")} />
            trace · {message.steps.length} step{message.steps.length === 1 ? "" : "s"}
            {message.sessionId ? (
              <span className="ml-auto normal-case tracking-normal text-faint">
                {message.sessionId}
              </span>
            ) : null}
          </button>
          {open
            ? message.steps.map((step) => <StepRow key={`${message.id}-${step.step}`} step={step} />)
            : null}
        </div>
      ) : null}
    </article>
  );
}

function Composer() {
  const { send, busy, messages, resetConversation } = useGenio();
  const [value, setValue] = useState("");
  const areaRef = useRef<HTMLTextAreaElement>(null);

  const submit = () => {
    if (!value.trim() || busy) return;
    const text = value;
    setValue("");
    void send(text);
  };

  return (
    <div className="border-t border-line/70 bg-panel2/40 p-3 sm:p-4">
      {messages.length === 0 ? (
        <div className="mb-3 flex flex-wrap gap-1.5">
          {SUGGESTIONS.map((suggestion) => (
            <button
              key={suggestion}
              type="button"
              onClick={() => setValue(suggestion)}
              className="rounded-full border border-line/80 px-2.5 py-1 font-mono text-[10px] text-muted transition hover:border-cyan/40 hover:text-cyan"
            >
              {suggestion}
            </button>
          ))}
        </div>
      ) : null}
      <div className="flex items-end gap-2">
        <textarea
          ref={areaRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          rows={2}
          maxLength={4000}
          placeholder={
            busy
              ? "Genio is running a cycle…"
              : "Ask the machine, read a document, request a privileged action…"
          }
          className="scroll min-h-[52px] flex-1 resize-none rounded-lg border border-line bg-void/60 px-3 py-2.5 font-mono text-[12px] leading-relaxed text-ink outline-none transition placeholder:text-faint focus:border-cyan/40 focus:ring-1 focus:ring-cyan/20"
          disabled={busy}
        />
        <div className="flex flex-col gap-1">
          <button
            type="button"
            onClick={submit}
            disabled={busy || !value.trim()}
            className="btn btn-primary h-[34px] px-3"
            aria-label="send message"
          >
            {busy ? <Spinner className="h-3.5 w-3.5" /> : <IconSend className="h-3.5 w-3.5" />}
            send
          </button>
          <button
            type="button"
            onClick={resetConversation}
            className="btn btn-ghost h-[26px] px-2 text-[10px]"
            disabled={busy || messages.length === 0}
          >
            clear
          </button>
        </div>
      </div>
      <p className="mt-2 font-mono text-[10px] text-faint">
        enter to send · shift+enter newline · model output is untrusted, tools are brokered
      </p>
    </div>
  );
}

function TraceEvent({ event }: { event: GenioEvent }) {
  const content = (() => {
    switch (event.phase) {
      case "policy":
        return (
          <span className="flex flex-wrap items-center gap-1.5">
            <span className="text-ink">{event.tool}</span>
            <span className={cx("chip", decisionClass(event.decision))}>
              {decisionLabel(event.decision)}
            </span>
            {event.rule ? <span className="text-faint">{event.rule}</span> : null}
          </span>
        );
      case "act":
        return (
          <span className="break-all text-ink/90">
            {event.tool}
            {event.args ? (
              <span className="text-muted"> {JSON.stringify(event.args)}</span>
            ) : null}
          </span>
        );
      case "observation":
        return (
          <span className={cx("break-words", event.ok ? "text-muted" : "text-rose/90")}>
            {event.tool}: {event.summary ?? (event.code as string) ?? "—"}
          </span>
        );
      case "approval_required":
        return (
          <span className="text-amber">
            {event.tool} · awaiting human authorization ({event.approval_id})
          </span>
        );
      case "final":
        return <span className="text-mint">{event.answer ?? "final answer"}</span>;
      case "error":
        return <span className="text-rose">{event.detail ?? event.code ?? "error"}</span>;
      case "plan":
      case "thought":
        return <span className="text-ink/80">{event.thought ?? event.detail ?? "planning"}</span>;
      default:
        return (
          <span className="text-muted">
            {event.detail ?? event.thought ?? event.tool ?? JSON.stringify(event).slice(0, 160)}
          </span>
        );
    }
  })();

  return (
    <li className="flex gap-2 px-3 py-1.5 font-mono text-[10px] leading-relaxed">
      <span className="shrink-0 text-faint">{clock(event.ts)}</span>
      <span
        className={cx(
          "w-[74px] shrink-0 uppercase tracking-[0.1em]",
          event.phase === "policy"
            ? "text-amber/90"
            : event.phase === "observation"
              ? "text-cyan/80"
              : event.phase === "error"
                ? "text-rose"
                : event.phase === "final"
                  ? "text-mint"
                  : "text-muted",
        )}
      >
        {phaseLabel(event.phase)}
      </span>
      <span className="min-w-0 flex-1">{content}</span>
    </li>
  );
}

function LiveTrace() {
  const { events, activeSessionId, messages } = useGenio();
  const bottomRef = useRef<HTMLDivElement>(null);

  const trace = useMemo(() => {
    const agentEvents = events.filter((event) => event.type === "agent");
    if (!activeSessionId) return agentEvents.slice(-60);
    return agentEvents.filter((event) => event.session_id === activeSessionId).slice(-120);
  }, [events, activeSessionId]);

  /* If the SSE stream was reconnecting when a turn ran, rebuild the timeline
     from the authoritative POST response rather than showing an empty panel. */
  const fallbackSteps = useMemo(() => {
    if (trace.length > 0 || !activeSessionId) return [];
    const message = [...messages]
      .reverse()
      .find((item) => item.sessionId === activeSessionId && item.steps?.length);
    return message?.steps ?? [];
  }, [activeSessionId, messages, trace.length]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "nearest" });
  }, [trace.length]);

  return (
    <Panel className="flex max-h-[420px] flex-col">
      <PanelHead
        title="live trace"
        hint={
          trace.length === 0 && fallbackSteps.length > 0
            ? `${activeSessionId} · rebuilt from response`
            : (activeSessionId ?? "no active session")
        }
        right={
          <Badge tone="cyan">
            {trace.length > 0 ? `${trace.length} events` : `${fallbackSteps.length} steps`}
          </Badge>
        }
      />
      <div className="scroll min-h-28 flex-1 overflow-y-auto py-1">
        {trace.length === 0 && fallbackSteps.length === 0 ? (
          <Empty title="no events yet" hint="Send a message — perceive, plan, policy, act and observe stream here." />
        ) : trace.length > 0 ? (
          <ul>
            {trace.map((event, index) => (
              <TraceEvent key={`${event.ts ?? "e"}-${index}`} event={event} />
            ))}
            <div ref={bottomRef} />
          </ul>
        ) : (
          <div>
            {fallbackSteps.map((step) => (
              <StepRow key={`fallback-${step.step}`} step={step} />
            ))}
            <div ref={bottomRef} />
          </div>
        )}
      </div>
    </Panel>
  );
}

function ApprovalQueue() {
  const { pending, focusApproval } = useGenio();
  return (
    <Panel>
      <PanelHead
        title="approval center"
        hint="privileged R2 queue"
        right={
          pending.length > 0 ? <Badge tone="amber">{pending.length}</Badge> : <Badge>empty</Badge>
        }
      />
      {pending.length === 0 ? (
        <p className="px-4 py-3 font-mono text-[10px] text-faint">
          no pending requests — R2 actions pause here before execution.
        </p>
      ) : (
        <ul className="divide-y divide-line/60">
          {pending.map((item) => (
            <li key={item.id} className="flex items-center gap-2 px-4 py-2.5">
              <IconShield className="h-3.5 w-3.5 shrink-0 text-amber" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-mono text-[11px] text-ink">{item.tool}</p>
                <p className="truncate font-mono text-[10px] text-faint">{item.intent}</p>
              </div>
              <span className="mono text-[10px] text-muted">{item.expires_in_s}s</span>
              <button type="button" className="btn px-2 py-1 text-[10px]" onClick={() => focusApproval(item)}>
                review
              </button>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

function Vitals() {
  const { telemetry, history, status } = useGenio();
  const cpu = telemetry?.cpu?.percent ?? history.at(-1)?.cpu_percent ?? null;
  const mem = telemetry?.memory?.percent ?? history.at(-1)?.mem_percent ?? null;
  const services = telemetry?.services?.running_count ?? history.at(-1)?.services_running ?? null;
  const failed = telemetry?.services?.failed_count ?? 0;

  const rows = [
    {
      label: "cpu",
      value: pct(cpu),
      width: cpu ?? 0,
      color: (cpu ?? 0) > 85 ? "bg-rose" : (cpu ?? 0) > 65 ? "bg-amber" : "bg-cyan",
      icon: IconActivity,
    },
    {
      label: "memory",
      value: pct(mem),
      width: mem ?? 0,
      color: (mem ?? 0) > 85 ? "bg-rose" : (mem ?? 0) > 65 ? "bg-amber" : "bg-mint",
      icon: IconTerminal,
    },
    {
      label: "services",
      value: services === null ? "—" : String(services),
      width: failed > 0 ? 100 : 55,
      color: failed > 0 ? "bg-rose" : "bg-cyan/70",
      icon: IconShield,
    },
  ];

  return (
    <Panel>
      <PanelHead title="vitals" hint={status === "degraded" ? "degraded reading" : "live telemetry"} />
      <div className="space-y-3 px-4 py-3">
        {rows.map((row) => (
          <div key={row.label}>
            <div className="mb-1 flex items-center gap-2">
              <row.icon className="h-3 w-3 text-faint" />
              <span className="label">{row.label}</span>
              <span className="mono ml-auto text-[11px] text-ink">{row.value}</span>
            </div>
            <div className="h-1 overflow-hidden rounded-full bg-panel2">
              <div className={cx("h-full transition-all duration-700", row.color)} style={{ width: `${Math.min(100, row.width)}%` }} />
            </div>
          </div>
        ))}
        {failed > 0 ? (
          <p className="flex items-center gap-1.5 font-mono text-[10px] text-rose">
            <IconAlert className="h-3 w-3" />
            {failed} failed unit{failed === 1 ? "" : "s"}
          </p>
        ) : (
          <p className="flex items-center gap-1.5 font-mono text-[10px] text-mint/80">
            <IconCheck className="h-3 w-3" /> no failed units
          </p>
        )}
      </div>
    </Panel>
  );
}

export function ConsoleView() {
  const { messages, busy, status, activeSessionId, health } = useGenio();
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [messages]);

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px] xl:grid-cols-[minmax(0,1fr)_390px]">
      <Panel className="flex min-h-[calc(100vh-230px)] flex-col">
        <PanelHead
          title="console"
          hint={
            busy
              ? "agent cycle in flight"
              : messages.length === 0
                ? "direct line to the daemon"
                : `${messages.length} messages · ${activeSessionId ?? "—"}`
          }
          right={
            <>
              <Badge tone={status === "degraded" ? "rose" : "mint"}>
                {health?.mind ? "mind online" : "mind"}
              </Badge>
              <Badge tone="line">phase {health?.phase ?? "—"}</Badge>
            </>
          }
        />
        <div ref={scrollRef} className="scroll flex-1 divide-y divide-line/50 overflow-y-auto">
          {messages.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-3 px-6 py-16 text-center">
              <span className="grid h-12 w-12 place-items-center rounded-xl border border-cyan/25 bg-cyan/5 text-cyan">
                <IconTerminal className="h-6 w-6" />
              </span>
              <h3 className="font-mono text-sm tracking-[0.18em] text-ink">GENIO IS LISTENING</h3>
              <p className="max-w-md text-xs leading-relaxed text-muted">
                Ask about the host, read a document from the drive, or request a privileged
                action. Every proposal is evaluated by policy before execution, and R2
                actions pause for your signature.
              </p>
              <div className="mt-1 flex flex-wrap justify-center gap-1.5 font-mono text-[10px] text-faint">
                <span className="chip">perceive</span>
                <span className="chip">plan</span>
                <span className="chip">policy</span>
                <span className="chip">act</span>
                <span className="chip">observe</span>
                <span className="chip">reflect</span>
              </div>
            </div>
          ) : (
            messages.map((message) => <Message key={message.id} message={message} />)
          )}
        </div>
        <Composer />
      </Panel>

      <div className="space-y-4">
        <LiveTrace />
        <ApprovalQueue />
        <Vitals />
      </div>
    </div>
  );
}

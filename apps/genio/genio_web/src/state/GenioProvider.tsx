import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { api, ApiError, ensureSession, loadSession } from "../lib/api";
import type {
  AgentResponse,
  ApprovalInfo,
  ChatMessage,
  GenioEvent,
  GenioStatus,
  Health,
  PendingApproval,
  PolicySnapshot,
  Sample,
  SessionInfo,
  Telemetry,
  ToolInfo,
} from "../lib/types";
import { newSessionId, uid } from "../lib/util";

const MAX_EVENTS = 320;
const MAX_HISTORY = 120;

interface GenioContextValue {
  health: Health | null;
  telemetry: Telemetry | null;
  history: Sample[];
  events: GenioEvent[];
  pending: PendingApproval[];
  policy: PolicySnapshot | null;
  tools: ToolInfo[];
  session: SessionInfo | null;
  messages: ChatMessage[];
  busy: boolean;
  deciding: boolean;
  connected: boolean;
  status: GenioStatus;
  approval: ApprovalInfo | null;
  activeSessionId: string | null;
  error: string | null;
  send: (text: string) => Promise<void>;
  grant: () => Promise<void>;
  deny: () => Promise<void>;
  dismissApproval: () => void;
  focusApproval: (approval: PendingApproval) => void;
  refreshCatalogue: () => Promise<void>;
  refreshTelemetry: (fresh?: boolean) => Promise<void>;
  clearError: () => void;
  resetConversation: () => void;
}

const GenioContext = createContext<GenioContextValue | null>(null);

function messageFor(err: unknown): string {
  if (err instanceof ApiError) return err.message;
  if (err instanceof Error) return err.message;
  return String(err);
}

export function GenioProvider({ children }: { children: ReactNode }) {
  const [health, setHealth] = useState<Health | null>(null);
  const [telemetry, setTelemetry] = useState<Telemetry | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [events, setEvents] = useState<GenioEvent[]>([]);
  const [pending, setPending] = useState<PendingApproval[]>([]);
  const [policy, setPolicy] = useState<PolicySnapshot | null>(null);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [session, setSession] = useState<SessionInfo | null>(() => loadSession());
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [busy, setBusy] = useState(false);
  const [deciding, setDeciding] = useState(false);
  const [connected, setConnected] = useState(false);
  const [queue, setQueue] = useState<ApprovalInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  const activeSession = useRef<string | null>(null);
  const queueRef = useRef<ApprovalInfo[]>([]);
  queueRef.current = queue;

  const enqueueApproval = useCallback((approval: ApprovalInfo) => {
    if (!approval?.approval_id) return;
    setQueue((prev) =>
      prev.some((item) => item.approval_id === approval.approval_id)
        ? prev
        : [...prev, approval],
    );
  }, []);

  const refreshPending = useCallback(async () => {
    try {
      const data = await api.pendingApprovals();
      setPending(data.approvals);
    } catch {
      /* polling is best-effort; the SSE stream is the primary signal */
    }
  }, []);

  const refreshTelemetry = useCallback(async (fresh = false) => {
    try {
      const snap = await api.telemetry(fresh);
      setTelemetry(snap);
    } catch (err) {
      setError(messageFor(err));
    }
  }, []);

  const refreshCatalogue = useCallback(async () => {
    try {
      const [pol, toolData] = await Promise.all([api.policy(), api.tools()]);
      setPolicy(pol);
      setTools(toolData.tools);
    } catch (err) {
      setError(messageFor(err));
    }
  }, []);

  /* ------------------------------------------------------------------ boot */
  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [h, snap, status, pol, toolData] = await Promise.all([
          api.health(),
          api.telemetry(),
          api.agentStatus(),
          api.policy(),
          api.tools(),
        ]);
        if (!alive) return;
        setHealth(h);
        setTelemetry(snap);
        setHistory(status.ring.slice(-MAX_HISTORY));
        setPending(status.pending_approvals ?? []);
        setPolicy(pol);
        setTools(toolData.tools);
      } catch (err) {
        if (alive) setError(messageFor(err));
      }
      try {
        const info = await ensureSession();
        if (alive) setSession(info);
      } catch (err) {
        if (alive) setError(`session: ${messageFor(err)}`);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  /* ------------------------------------------------------------- polling */
  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshPending();
      api
        .agentStatus()
        .then((status) => {
          setHealth((prev) =>
            prev
              ? {
                  ...prev,
                  agent: { ...prev.agent, ...status },
                }
              : prev,
          );
        })
        .catch(() => undefined);
    }, 6000);
    return () => window.clearInterval(timer);
  }, [refreshPending]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      api
        .health()
        .then(setHealth)
        .catch(() => undefined);
    }, 15_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshTelemetry(false);
    }, 20_000);
    return () => window.clearInterval(timer);
  }, [refreshTelemetry]);

  /* ------------------------------------------------------------------ SSE */
  useEffect(() => {
    const source = new EventSource("/agent/events");

    const handle = (eventName: string) => (raw: MessageEvent<string>) => {
      let payload: GenioEvent;
      try {
        payload = JSON.parse(raw.data) as GenioEvent;
      } catch {
        return;
      }
      const event: GenioEvent = { ...payload, type: (payload.type ?? eventName) as GenioEvent["type"] };
      setEvents((prev) => [...prev.slice(-(MAX_EVENTS - 1)), event]);

      if (event.type === "tick") {
        const sample: Sample = {
          ts: event.ts ?? new Date().toISOString(),
          cpu_percent: event.cpu_percent ?? null,
          mem_percent: event.mem_percent ?? null,
          services_running: event.services_running ?? null,
          collector_ms: event.collector_ms,
        };
        setHistory((prev) => [...prev.slice(-(MAX_HISTORY - 1)), sample]);
        setTelemetry((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            timestamp: sample.ts,
            cpu: prev.cpu ? { ...prev.cpu, percent: sample.cpu_percent } : prev.cpu,
            memory: prev.memory
              ? { ...prev.memory, percent: sample.mem_percent ?? prev.memory.percent }
              : prev.memory,
            services: prev.services
              ? { ...prev.services, running_count: sample.services_running ?? prev.services.running_count }
              : prev.services,
          };
        });
      }

      if (event.type === "agent" && event.phase === "approval_required" && event.approval_id) {
        enqueueApproval({
          approval_id: event.approval_id,
          tool: event.tool ?? "unknown",
          args: event.args,
          risk: event.risk,
          prompt: event.prompt ?? null,
          expires_in_s: event.expires_in_s,
          policy: event.policy,
          session_id: event.session_id,
        });
        void refreshPending();
      }

      if (event.type === "approval") {
        void refreshPending();
      }
    };

    const listeners: Array<[string, (event: MessageEvent<string>) => void]> = [
      ["agent", handle("agent")],
      ["tick", handle("tick")],
      ["tool", handle("tool")],
      ["approval", handle("approval")],
      ["hello", handle("hello")],
      ["message", handle("message")],
    ];
    listeners.forEach(([name, fn]) => source.addEventListener(name, fn as EventListener));

    source.onopen = () => setConnected(true);
    source.onerror = () => setConnected(false);

    return () => {
      listeners.forEach(([name, fn]) => source.removeEventListener(name, fn as EventListener));
      source.close();
    };
  }, [enqueueApproval, refreshPending]);

  /* --------------------------------------------------------------- actions */
  const patchAssistant = useCallback((sessionId: string, patch: Partial<ChatMessage>) => {
    setMessages((prev) =>
      prev.map((message) =>
        message.role === "assistant" && message.sessionId === sessionId
          ? { ...message, ...patch }
          : message,
      ),
    );
  }, []);

  const applyResponse = useCallback(
    (sessionId: string, response: AgentResponse) => {
      patchAssistant(sessionId, {
        status: response.status,
        text: response.answer,
        steps: response.steps,
        degraded: response.degraded,
        error: response.error,
      });
    },
    [patchAssistant],
  );

  const send = useCallback(
    async (text: string) => {
      const content = text.trim();
      if (!content || busy) return;
      const sessionId = newSessionId();
      const assistantId = uid();
      activeSession.current = sessionId;

      setMessages((prev) => [
        ...prev,
        { id: uid(), role: "user", text: content, ts: Date.now() },
        {
          id: assistantId,
          role: "assistant",
          text: "",
          ts: Date.now(),
          sessionId,
          status: "thinking",
        },
      ]);
      setBusy(true);
      setError(null);

      try {
        const response = await api.sendMessage(content, sessionId);
        applyResponse(sessionId, response);
        if (response.approval) enqueueApproval({ ...response.approval, session_id: sessionId });
      } catch (err) {
        patchAssistant(sessionId, {
          status: "error",
          error: messageFor(err),
          text: `Daemon error — ${messageFor(err)}`,
        });
        setError(messageFor(err));
      } finally {
        setBusy(false);
        activeSession.current = null;
        void refreshPending();
      }
    },
    [busy, enqueueApproval, applyResponse, patchAssistant, refreshPending],
  );

  const grant = useCallback(async () => {
    const approval = queueRef.current[0];
    if (!approval || deciding) return;
    setDeciding(true);
    setError(null);
    try {
      const granted = await api.grantApproval(approval.approval_id);
      if (approval.session_id) {
        const response = await api.resumeAgent(approval.session_id, granted.approval_token);
        applyResponse(approval.session_id, response);
        if (response.approval) {
          enqueueApproval({ ...response.approval, session_id: approval.session_id });
        }
      }
      setQueue((prev) => prev.filter((item) => item.approval_id !== approval.approval_id));
      void refreshPending();
    } catch (err) {
      setError(`approval: ${messageFor(err)}`);
    } finally {
      setDeciding(false);
    }
  }, [applyResponse, deciding, enqueueApproval, refreshPending]);

  const deny = useCallback(async () => {
    const approval = queueRef.current[0];
    if (!approval || deciding) return;
    setDeciding(true);
    setError(null);
    try {
      await api.denyApproval(approval.approval_id, approval.session_id);
      if (approval.session_id) {
        patchAssistant(approval.session_id, {
          status: "denied",
          text: "Action denied by the operator. Nothing was executed.",
        });
      }
      setQueue((prev) => prev.filter((item) => item.approval_id !== approval.approval_id));
      void refreshPending();
    } catch (err) {
      setError(`denial: ${messageFor(err)}`);
    } finally {
      setDeciding(false);
    }
  }, [deciding, patchAssistant, refreshPending]);

  const dismissApproval = useCallback(() => {
    setQueue((prev) => prev.slice(1));
  }, []);

  const focusApproval = useCallback(
    (item: PendingApproval) => {
      enqueueApproval({
        approval_id: item.id,
        tool: item.tool,
        risk: item.risk,
        expires_in_s: item.expires_in_s,
        intent: item.intent,
      });
    },
    [enqueueApproval],
  );

  const resetConversation = useCallback(() => {
    setMessages([]);
    setQueue([]);
    setEvents([]);
    activeSession.current = null;
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const activeSessionId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (message.role === "assistant" && message.sessionId) return message.sessionId;
    }
    return null;
  }, [messages]);

  const status: GenioStatus = useMemo(() => {
    if (queue.length > 0 || messages.some((m) => m.status === "awaiting_approval")) {
      return "awaiting_approval";
    }
    if (busy) return "thinking";
    if (!connected || !health || health.status !== "ok" || health.agent.degraded) {
      return "degraded";
    }
    return "idle";
  }, [busy, connected, health, messages, queue.length]);

  const value: GenioContextValue = {
    health,
    telemetry,
    history,
    events,
    pending,
    policy,
    tools,
    session,
    messages,
    busy,
    deciding,
    connected,
    status,
    approval: queue[0] ?? null,
    activeSessionId,
    error,
    send,
    grant,
    deny,
    dismissApproval,
    focusApproval,
    refreshCatalogue,
    refreshTelemetry,
    clearError,
    resetConversation,
  };

  return <GenioContext.Provider value={value}>{children}</GenioContext.Provider>;
}

export function useGenio(): GenioContextValue {
  const ctx = useContext(GenioContext);
  if (!ctx) throw new Error("useGenio must be used inside GenioProvider");
  return ctx;
}

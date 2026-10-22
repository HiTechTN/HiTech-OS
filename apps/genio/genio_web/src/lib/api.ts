import type {
  AgentResponse,
  AgentStatus,
  AuditRecord,
  AuditVerify,
  Health,
  PendingApproval,
  PolicySnapshot,
  PortalInfo,
  SessionInfo,
  Telemetry,
  ToolInfo,
} from "./types";

/**
 * Empty base = same origin. In dev, Vite proxies the daemon routes; in
 * production the daemon serves the built SPA at `/ui/` (same origin).
 * Set VITE_GENIO_API to an absolute URL when behind a reverse proxy.
 */
const BASE = (import.meta.env.VITE_GENIO_API as string | undefined)?.replace(/\/$/, "") ?? "";
const SESSION_KEY = "genio.session.v1";

export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

async function request<T>(
  path: string,
  init?: RequestInit,
  /** Agent turns report failures as a valid JSON body with HTTP 500. */
  acceptErrorBody = false,
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, {
      ...init,
      headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
    });
  } catch (err) {
    throw new ApiError(`daemon unreachable (${(err as Error).message})`, 0);
  }
  const text = await res.text();
  let body: unknown = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    /* non-JSON body */
  }
  if (!res.ok) {
    if (acceptErrorBody && body && typeof body === "object" && "status" in body) {
      return body as T;
    }
    const envelope = body as { detail?: unknown; message?: unknown } | null;
    const detail = envelope?.detail ?? envelope?.message;
    throw new ApiError(
      typeof detail === "string" ? detail : res.statusText || `HTTP ${res.status}`,
      res.status,
    );
  }
  return body as T;
}

function post<T>(path: string, body: unknown, acceptErrorBody = false): Promise<T> {
  return request<T>(path, { method: "POST", body: JSON.stringify(body) }, acceptErrorBody);
}

export const api = {
  health: () => request<Health>("/health"),
  telemetry: (fresh = false) => request<Telemetry>(`/telemetry${fresh ? "?fresh=true" : ""}`),
  agentStatus: () => request<AgentStatus>("/agent/status"),
  tools: () => request<{ count: number; tools: ToolInfo[] }>("/tools"),
  policy: () => request<PolicySnapshot>("/policy"),
  pendingApprovals: () => request<{ count: number; approvals: PendingApproval[] }>("/approvals"),
  portalStatus: () => request<PortalInfo>("/portal/status"),
  auditVerify: () => request<AuditVerify>("/audit/verify"),
  auditTail: (n = 40) => request<{ records: AuditRecord[] }>(`/audit/tail?n=${n}`),

  createSession: (subject = "web-gui") =>
    post<{ token: string; caps: string[]; expires_in_s: number }>("/caps/session", { subject }),

  sendMessage: (message: string, sessionId: string) =>
    post<AgentResponse>("/agent/message", { message, session_id: sessionId }, true),

  resumeAgent: (sessionId: string, approvalToken: string) =>
    post<AgentResponse>("/agent/message", {
      session_id: sessionId,
      approval_token: approvalToken,
    }, true),

  grantApproval: (approvalId: string, approver = "web-human") =>
    post<{ ok: boolean; approval_token: string; expires_in_s: number }>(
      `/approvals/${encodeURIComponent(approvalId)}/grant`,
      { approver },
    ),

  denyApproval: (approvalId: string, sessionId?: string, approver = "web-human") =>
    post<{ ok: boolean; denied: boolean; cancelled: boolean }>(
      `/approvals/${encodeURIComponent(approvalId)}/deny`,
      { approver, session_id: sessionId ?? null },
    ),

  /** Stateless what-if — never executes anything. */
  evaluatePolicy: (tool: string, caps: string[] | null, args?: Record<string, unknown>) =>
    post<Record<string, unknown> & { decision: string; code: string }>("/policy/evaluate", {
      tool,
      caps,
      args,
    }),
};

export function loadSession(): SessionInfo | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SessionInfo;
    if (!parsed?.token || parsed.expires_at < Date.now() + 15_000) return null;
    return parsed;
  } catch {
    return null;
  }
}

export async function ensureSession(): Promise<SessionInfo> {
  const existing = loadSession();
  if (existing) return existing;
  const res = await api.createSession();
  const session: SessionInfo = {
    token: res.token,
    caps: res.caps,
    expires_at: Date.now() + res.expires_in_s * 1000,
  };
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
  return session;
}

export function clearSession(): void {
  localStorage.removeItem(SESSION_KEY);
}

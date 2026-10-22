/** Wire types for the Genio daemon API (v0.4.0-phase4-5). */

export type Risk = "R0" | "R1" | "R2";
export type Decision = "ALLOW" | "DENY" | "REQUIRE_CONFIRMATION";
export type GenioStatus = "idle" | "thinking" | "awaiting_approval" | "degraded";

export interface ToolParam {
  type: string;
  required?: boolean;
  min?: number;
  max?: number;
}

export interface ToolInfo {
  id: string;
  risk: Risk;
  description: string;
  cap: string;
  params: Record<string, ToolParam>;
  rate_per_min: number;
  tags: string[];
  requires_approval?: boolean;
}

export interface AuditInfo {
  audit_writable: boolean;
  audit_seq: number;
  pending_approvals: number;
  state_dir: string;
  tools: number;
}

export interface AgentStatus {
  loop_running: boolean;
  ticks: number;
  errors: number;
  last_tick: string | null;
  last_tick_ms: number | null;
  degraded: boolean;
  tick_seconds: number;
  ring: Sample[];
  pending_approvals: PendingApproval[];
}

export interface RegistrySummary {
  count: number;
  by_risk: Record<string, number>;
  capabilities: string[];
}

export interface PolicySummary {
  rules: number;
  deny_tools: string[];
  private_capability: string;
  write_roots: string[];
}

export interface PortalInfo {
  base_url?: string;
  auth_mode?: string;
  status?: string;
  service?: string;
  ok?: boolean;
}

export interface Health {
  status: string;
  service: string;
  version: string;
  phase: number;
  uptime_s: number;
  agent: AgentStatus;
  broker: AuditInfo;
  registry: RegistrySummary;
  policy: PolicySummary;
  portal: PortalInfo;
  mind?: Record<string, unknown>;
}

export interface TelemetryHost {
  hostname: string;
  kernel: string;
  machine: string;
  python: string;
  nixos_generation: string | null;
}

export interface TelemetryCpu {
  count: number;
  physical: number | null;
  percent: number | null;
  per_core: number[] | null;
  freq_mhz: number | null;
}

export interface TelemetryMemory {
  total_mb: number;
  used_mb: number;
  available_mb: number;
  percent: number;
  swap_total_mb: number;
  swap_used_mb: number;
}

export interface DiskInfo {
  total_gb: number;
  used_gb: number;
  free_gb: number;
  percent: number | null;
}

export interface TelemetryServices {
  running_count?: number;
  failed_count?: number;
  failed_units?: string[];
  sample?: string[];
  error?: string;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  rss_mb: number | null;
  cpu_percent: number | null;
}

export interface Telemetry {
  schema: string;
  timestamp: string;
  host: TelemetryHost;
  uptime_s: number | null;
  load: { "1m": number; "5m": number; "15m": number } | null;
  cpu: TelemetryCpu | null;
  memory: TelemetryMemory | null;
  disks: Record<string, DiskInfo> | null;
  services: TelemetryServices | null;
  processes: { count: number | null; top_by_rss: ProcessInfo[] } | null;
  sensors: Record<string, number> | null;
  collector_ms: number;
  error?: string;
}

export interface Sample {
  ts: string;
  cpu_percent: number | null;
  mem_percent: number | null;
  services_running: number | null;
  collector_ms?: number;
}

export interface AgentStep {
  step: number;
  thought: string;
  tool: string | null;
  args: Record<string, unknown>;
  decision: string | null;
  ok: boolean;
  audit_seq: number | null;
  observation: string;
}

export interface PolicyVerdict {
  decision: Decision;
  code: string;
  reason: string;
  tool: string;
  risk: Risk;
  capability: string;
  rule: string;
}

export interface ApprovalInfo {
  approval_id: string;
  tool: string;
  args?: Record<string, unknown>;
  risk?: string;
  prompt?: string | null;
  expires_in_s?: number;
  policy?: PolicyVerdict;
  session_id?: string;
  intent?: string;
}

export interface AgentResponse {
  status: "completed" | "awaiting_approval" | "error";
  answer: string;
  session_id: string;
  steps: AgentStep[];
  steps_used: number;
  max_steps: number;
  degraded: boolean;
  approval: ApprovalInfo | null;
  error: string | null;
  router?: Record<string, unknown>;
}

export interface PendingApproval {
  id: string;
  tool: string;
  intent: string;
  risk: string;
  expires_in_s: number;
}

export interface PolicyRule {
  id: string;
  decision: Decision;
  description: string;
}

export interface MatrixRow {
  decision: Decision;
  code: string;
  reason: string;
  tool: string;
  risk: string;
  capability: string;
  rule: string;
}

export interface PolicySnapshot {
  rules: PolicyRule[];
  deny_tools: string[];
  deny_subjects: string[];
  private_capability: string;
  write_roots: string[];
  matrix: MatrixRow[];
}

export interface AuditVerify {
  ok: boolean;
  records: number;
  head?: string;
  seq?: number;
  path?: string;
  error?: string;
}

export interface AuditRecord {
  seq: number;
  ts: string;
  actor: string;
  tool: string;
  args_hash: string;
  decision: string;
  detail: string;
  prev: string;
  hash: string;
}

export interface SessionInfo {
  token: string;
  caps: string[];
  expires_at: number;
}

/** A unified SSE event (`type` is the SSE event name). */
export interface GenioEvent {
  type: "agent" | "tick" | "tool" | "approval" | "hello" | "message";
  ts?: string;
  // agent
  phase?: string;
  session_id?: string;
  step?: number;
  thought?: string;
  action?: unknown;
  detail?: string;
  decision?: string;
  code?: string;
  rule?: string;
  tool?: string;
  args?: Record<string, unknown>;
  ok?: boolean;
  summary?: string;
  answer?: string;
  degraded?: boolean;
  approval_id?: string;
  risk?: string;
  prompt?: string | null;
  expires_in_s?: number;
  policy?: PolicyVerdict;
  approver?: string;
  // tick
  cpu_percent?: number | null;
  mem_percent?: number | null;
  services_running?: number | null;
  collector_ms?: number;
  // hello / misc
  version?: string;
  [key: string]: unknown;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  ts: number;
  sessionId?: string;
  status?: AgentResponse["status"] | "thinking" | "denied";
  steps?: AgentStep[];
  degraded?: boolean;
  error?: string | null;
}

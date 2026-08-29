import type {
  CandidateView,
  CreateRunRequest,
  DoctorReport,
  DurableEvent,
  RunDetail,
  RunSummary,
  TimelineEntry,
} from "@/generated/api-types";

export const API_BASE = "/api/v1";
const CSRF_HEADER = "x-heikas-csrf";
const BOOTSTRAP_HEADER = "x-heikas-bootstrap";
const CSRF_COOKIE = "heikas_csrf";

export interface ApiErrorBody {
  code: string;
  message: string;
  remedy: string | null;
  retryable: boolean;
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly remedy: string | null;
  readonly retryable: boolean;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = "ApiError";
    this.status = status;
    this.code = body.code;
    this.remedy = body.remedy;
    this.retryable = body.retryable;
  }
}

export interface HealthResponse {
  status: string;
  version: string;
  demonstration_mode: boolean;
  active_dispatches: number;
}

export interface SessionResponse {
  csrf_token: string;
  demonstration_mode: boolean;
}

export interface AgentDriverDescription {
  id: string;
  label: string;
  requires_paid_account: boolean;
  demonstration_only: boolean;
}

export interface ConfigurationResponse {
  heikas_home: string;
  user_configuration_path: string;
  demonstration_mode: boolean;
  default_candidate_count: number;
  maximum_candidate_count: number;
  agent_drivers: AgentDriverDescription[];
  quality_profiles: string[];
  commit_policies: string[];
  recent_repositories: string[];
}

export interface GraphDefinition {
  nodes: {
    id: string;
    label: string;
    scope: string;
    class: string;
    read_only: boolean;
  }[];
  edges: { from: string; to: string; label: string }[];
}

export interface PlanValidation {
  missing_headings: string[];
  empty_sections: string[];
  expected_files: string[];
}

export interface PlanResponse {
  version: number | null;
  markdown: string | null;
  history: {
    versions: {
      version: number;
      hash: string;
      created_at: string;
      author: string;
      revision_note: string | null;
      byte_length: number;
    }[];
    approval: {
      decision: string;
      plan_version: number;
      plan_hash: string;
      decided_at: string;
      local_user: string;
      note: string | null;
    } | null;
  };
  approved: boolean;
  validation: PlanValidation | null;
  candidate_work_started: boolean;
}

export interface AcknowledgementResponse {
  accepted: boolean;
  detail: string;
}

export interface EventPage {
  run_id: string;
  events: DurableEvent[];
  next_sequence: number;
  complete: boolean;
}

export interface StructuredLogRecord {
  recorded_at: string;
  level: string;
  target: string;
  message: string;
  run_id: string | null;
  candidate_id: string | null;
  node_id: string | null;
  attempt: number | null;
  fields: unknown;
}

export interface LogPage {
  run_id: string;
  offset: number;
  total: number;
  records: StructuredLogRecord[];
}

export interface ExportResponse {
  archive_path: string;
  byte_length: number;
  entry_count: number;
  redacted: boolean;
}

let csrfToken: string | null = null;

export function readCookie(name: string): string | null {
  const match = document.cookie
    .split(";")
    .map((entry) => entry.trim().split("="))
    .find((entry) => entry[0] === name);
  return match && match[1] !== undefined ? decodeURIComponent(match[1]) : null;
}

export function currentCsrfToken(): string | null {
  return csrfToken ?? readCookie(CSRF_COOKIE);
}

export function setCsrfToken(token: string | null): void {
  csrfToken = token;
}

function decodeErrorBody(response: Response, text: string): ApiErrorBody {
  const unstructured: ApiErrorBody = {
    code: `http_${String(response.status)}`,
    message: text.length > 0 ? text : response.statusText,
    remedy: null,
    retryable: response.status >= 500,
  };
  try {
    return JSON.parse(text) as ApiErrorBody;
  } catch {
    return unstructured;
  }
}

async function parse<T>(response: Response): Promise<T> {
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(response.status, decodeErrorBody(response, text));
  }
  if (text.length === 0) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const method = init?.method ?? "GET";
  const headers = new Headers(init?.headers);
  if (init?.body !== undefined) {
    headers.set("content-type", "application/json");
  }
  if (method !== "GET" && method !== "HEAD") {
    const token = currentCsrfToken();
    if (token !== null) {
      headers.set(CSRF_HEADER, token);
    }
  }
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers,
    credentials: "same-origin",
  });
  return parse<T>(response);
}

export async function establishSession(bootstrapToken: string | null): Promise<SessionResponse> {
  const headers = new Headers();
  if (bootstrapToken !== null) {
    headers.set(BOOTSTRAP_HEADER, bootstrapToken);
  }
  const response = await fetch(`${API_BASE}/session`, {
    method: "POST",
    headers,
    credentials: "same-origin",
  });
  const session = await parse<SessionResponse>(response);
  setCsrfToken(session.csrf_token);
  return session;
}

export async function resumeSession(): Promise<SessionResponse> {
  const response = await fetch(`${API_BASE}/session`, {
    method: "GET",
    credentials: "same-origin",
  });
  const session = await parse<SessionResponse>(response);
  setCsrfToken(session.csrf_token);
  return session;
}

export async function openSession(bootstrapToken: string | null): Promise<SessionResponse> {
  if (bootstrapToken !== null) {
    return establishSession(bootstrapToken);
  }
  return resumeSession();
}

export const api = {
  health: () => request<HealthResponse>("/health"),
  configuration: () => request<ConfigurationResponse>("/config"),
  graph: () => request<GraphDefinition>("/graph"),
  doctor: (repositoryPath: string | null) =>
    request<DoctorReport>("/doctor", {
      method: "POST",
      body: JSON.stringify({ repository_path: repositoryPath }),
    }),
  listRuns: () => request<{ runs: RunSummary[] }>("/runs"),
  createRun: (payload: CreateRunRequest) =>
    request<{ run_id: string }>("/runs", { method: "POST", body: JSON.stringify(payload) }),
  runDetail: (runId: string) => request<RunDetail>(`/runs/${runId}`),
  resumeRun: (runId: string) =>
    request<AcknowledgementResponse>(`/runs/${runId}/resume`, { method: "POST" }),
  cancelRun: (runId: string, reason: string | null) =>
    request<AcknowledgementResponse>(`/runs/${runId}/cancel`, {
      method: "POST",
      body: JSON.stringify({ reason }),
    }),
  cleanupRun: (runId: string) =>
    request<AcknowledgementResponse>(`/runs/${runId}/cleanup`, { method: "POST" }),
  plan: (runId: string) => request<PlanResponse>(`/runs/${runId}/plan`),
  updatePlan: (runId: string, markdown: string) =>
    request<{ version: number; approval_invalidated: boolean }>(`/runs/${runId}/plan`, {
      method: "PUT",
      body: JSON.stringify({ markdown }),
    }),
  approvePlan: (runId: string, markdown: string | null, note: string | null) =>
    request<AcknowledgementResponse>(`/runs/${runId}/plan/approve`, {
      method: "POST",
      body: JSON.stringify({ markdown, note }),
    }),
  revisePlan: (runId: string, note: string) =>
    request<AcknowledgementResponse>(`/runs/${runId}/plan/revise`, {
      method: "POST",
      body: JSON.stringify({ note }),
    }),
  rejectPlan: (runId: string, reason: string | null) =>
    request<AcknowledgementResponse>(`/runs/${runId}/plan/reject`, {
      method: "POST",
      body: JSON.stringify({ reason }),
    }),
  approveCommit: (runId: string, note: string | null) =>
    request<AcknowledgementResponse>(`/runs/${runId}/commit/approve`, {
      method: "POST",
      body: JSON.stringify({ note }),
    }),
  candidates: (runId: string) =>
    request<{ candidates: CandidateView[] }>(`/runs/${runId}/candidates`),
  candidateDiff: (runId: string, candidateId: string) =>
    fetch(`${API_BASE}/runs/${runId}/candidates/${candidateId}/diff`, {
      credentials: "same-origin",
    }).then((response) => response.text()),
  integrationDiff: (runId: string) =>
    fetch(`${API_BASE}/runs/${runId}/integration/diff`, { credentials: "same-origin" }).then(
      (response) => response.text(),
    ),
  events: (runId: string, after: number, limit: number) =>
    request<EventPage>(`/runs/${runId}/events?after=${String(after)}&limit=${String(limit)}`),
  timeline: (runId: string) => request<{ entries: TimelineEntry[] }>(`/runs/${runId}/timeline`),
  logs: (runId: string, offset: number, limit: number) =>
    request<LogPage>(`/runs/${runId}/logs?offset=${String(offset)}&limit=${String(limit)}`),
  exportRun: (runId: string, includeWorktrees: boolean) =>
    request<ExportResponse>(`/runs/${runId}/export`, {
      method: "POST",
      body: JSON.stringify({ include_worktrees: includeWorktrees }),
    }),
};

export function eventStreamUrl(runId: string, afterSequence: number): string {
  return `${API_BASE}/runs/${runId}/stream?after=${String(afterSequence)}`;
}
